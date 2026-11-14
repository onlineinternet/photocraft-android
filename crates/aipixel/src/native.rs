// AI Pixel Studio for the Layerwise Android port of PhotoCraft.
// Licensed under the Apache License, Version 2.0 (LICENSE-APACHE),
// also offered under the MIT License (LICENSE-MIT) like the rest of this tree.

//! Blocking HTTP (rustls) and WebSocket. Callers run this on a worker thread, never on the UI thread.

use std::io::{ErrorKind, Read};
use std::net::TcpStream;
use std::time::Duration;

use tungstenite::client::IntoClientRequest;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

use crate::error::AiError;
use crate::transport::{HttpRequest, HttpResponse, Transport, WsConn, WsEvent};

pub struct NativeTransport;

impl Transport for NativeTransport {
    fn round_trip(&self, req: &HttpRequest) -> Result<HttpResponse, AiError> {
        let agent = ureq::AgentBuilder::new().timeout_connect(req.timeout).timeout_read(req.timeout).build();
        let mut call = agent.request(&req.method, &req.url);
        for (name, value) in &req.headers {
            call = call.set(name, value);
        }
        let result = if req.body.is_empty() { call.call() } else { call.send_bytes(&req.body) };
        match result {
            Ok(response) => read_response(response.status(), response.into_reader()),
            Err(ureq::Error::Status(status, response)) => read_response(status, response.into_reader()),
            Err(ureq::Error::Transport(err)) => Err(transport_error(&err)),
        }
    }

    fn open_ws(&self, url: &str, headers: &[(String, String)]) -> Result<Box<dyn WsConn>, AiError> {
        let mut request = url.into_client_request().map_err(|err| AiError::offline(format!("couldn't open the progress socket ({err})")))?;
        for (name, value) in headers {
            let header = tungstenite::http::header::HeaderName::from_bytes(name.as_bytes()).map_err(|_| AiError::bad("an auth header name isn't valid"))?;
            let header_value =
                tungstenite::http::header::HeaderValue::from_str(value).map_err(|_| AiError::auth("the auth value can't be sent as a header"))?;
            request.headers_mut().insert(header, header_value);
        }
        let (socket, _) = tungstenite::connect(request).map_err(|err| AiError::offline(format!("couldn't open the progress socket ({err})")))?;
        Ok(Box::new(NativeWs { socket }))
    }
}

fn read_response(status: u16, reader: impl Read) -> Result<HttpResponse, AiError> {
    let mut body = Vec::new();
    reader.take(16 * 1024 * 1024).read_to_end(&mut body).map_err(|err| {
        if err.kind() == ErrorKind::TimedOut {
            AiError::timeout("The server didn't answer in time.")
        } else {
            AiError::offline(format!("the download stopped ({err})"))
        }
    })?;
    Ok(HttpResponse { status, body })
}

fn transport_error(err: &ureq::Transport) -> AiError {
    let text = err.to_string();
    let lower = text.to_ascii_lowercase();
    if lower.contains("timed out") || lower.contains("timeout") {
        AiError::timeout("The server didn't answer in time.")
    } else {
        AiError::offline(format!("Couldn't reach the server. {text}"))
    }
}

struct NativeWs {
    socket: WebSocket<MaybeTlsStream<TcpStream>>,
}

impl WsConn for NativeWs {
    fn recv(&mut self, timeout: Duration) -> Result<WsEvent, AiError> {
        set_timeout(&self.socket, Some(timeout));
        let started = std::time::Instant::now();
        loop {
            if started.elapsed() > timeout.saturating_add(Duration::from_secs(1)) {
                return Ok(WsEvent::Timeout);
            }
            match self.socket.read() {
                Ok(Message::Text(text)) => return Ok(WsEvent::Text(text.to_string())),
                Ok(Message::Binary(bytes)) => return Ok(WsEvent::Text(String::from_utf8_lossy(&bytes).into_owned())),
                Ok(Message::Ping(payload)) => {
                    self.socket.send(Message::Pong(payload)).map_err(|err| AiError::offline(format!("the progress socket stopped ({err})")))?;
                }
                Ok(Message::Pong(_)) | Ok(Message::Frame(_)) => {}
                Ok(Message::Close(_)) => return Ok(WsEvent::Closed),
                Err(tungstenite::Error::Io(err)) if err.kind() == ErrorKind::WouldBlock || err.kind() == ErrorKind::TimedOut => return Ok(WsEvent::Timeout),
                Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => return Ok(WsEvent::Closed),
                Err(err) => return Err(AiError::offline(format!("the progress socket stopped ({err})"))),
            }
        }
    }
}

fn set_timeout(socket: &WebSocket<MaybeTlsStream<TcpStream>>, timeout: Option<Duration>) {
    let result = match socket.get_ref() {
        MaybeTlsStream::Plain(tcp) => tcp.set_read_timeout(timeout),
        MaybeTlsStream::Rustls(tls) => tls.get_ref().set_read_timeout(timeout),
        _ => Ok(()),
    };
    let _ = result;
}
