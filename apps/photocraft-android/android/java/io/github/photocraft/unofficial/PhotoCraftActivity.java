package io.github.photocraft.unofficial;

import android.app.NativeActivity;
import android.content.ClipData;
import android.content.ClipDescription;
import android.content.ClipboardManager;
import android.content.Context;
import android.content.ContentResolver;
import android.content.Intent;
import android.database.Cursor;
import android.net.Uri;
import android.os.Build;
import android.provider.MediaStore;
import android.provider.OpenableColumns;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.util.ArrayList;

/**
 * PhotoCraft's activity: Android's NativeActivity (which runs the Rust app, src/lib.rs) plus the
 * one thing it can't do, receiving the answer of Android's own file pickers.
 *
 * The Rust side (src/android_system.rs) calls {@link #openDocument}, {@link #pickPhotos} or
 * {@link #createDocument} through JNI, then polls {@link #takeResult} every frame. Opened files
 * are read here, on a worker thread, and handed over as bytes with their names; a file to save
 * comes back as a content:// URI that {@link #writeUri} writes to.
 */
public class PhotoCraftActivity extends NativeActivity {
    private static final int OPEN = 7101;
    private static final int SAVE = 7102;
    /** Files larger than this aren't read into memory (the app couldn't open them anyway). */
    private static final long MAX_BYTES = 1L << 30;

    private static final Object LOCK = new Object();
    /**
     * The latest picker answer, until the native side takes it:
     * {"open", String[] names, byte[][] data, String error-or-null, String[] uris}, {"save", uri,
     * name} or {"cancel"}. A uri is null when the file can't be reopened later (the photo picker
     * only lends its files).
     */
    private static Object[] result;

    private static void answer(Object[] r) {
        synchronized (LOCK) {
            result = r;
        }
    }

    /** The waiting answer, or null; each answer is returned once. */
    public static Object[] takeResult() {
        synchronized (LOCK) {
            Object[] r = result;
            result = null;
            return r;
        }
    }

    /**
     * {@link #takeResult} through the activity: a thread the native side attached can't look this
     * class up by name (it only sees the system's classes), but it has the activity.
     */
    public Object[] takeAnswer() {
        return takeResult();
    }

    /** Android's document picker (thumbnails, recent files, Drive, USB storage). */
    public void openDocument(final boolean multiple, final String[] mimeTypes) {
        runOnUiThread(() -> {
            Intent i = new Intent(Intent.ACTION_OPEN_DOCUMENT);
            i.addCategory(Intent.CATEGORY_OPENABLE);
            i.setType("*/*");
            if (mimeTypes != null && mimeTypes.length > 0) {
                i.putExtra(Intent.EXTRA_MIME_TYPES, mimeTypes);
            }
            i.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, multiple);
            // Keep access to the files, so File › Save writes back and Open Recent reopens them.
            i.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION
                    | Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION);
            launch(i, OPEN);
        });
    }

    /** Android's photo picker (Android 13+; older versions get the document picker for images). */
    public void pickPhotos(final boolean multiple) {
        runOnUiThread(() -> {
            Intent i;
            if (Build.VERSION.SDK_INT >= 33) {
                i = new Intent(MediaStore.ACTION_PICK_IMAGES);
                if (multiple) {
                    i.putExtra(MediaStore.EXTRA_PICK_IMAGES_MAX, MediaStore.getPickImagesMaxLimit());
                }
            } else {
                i = new Intent(Intent.ACTION_OPEN_DOCUMENT);
                i.addCategory(Intent.CATEGORY_OPENABLE);
                i.setType("image/*");
                i.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, multiple);
            }
            launch(i, OPEN);
        });
    }

    /** Android's "save to" picker, suggesting {@code name}. */
    public void createDocument(final String name, final String mimeType) {
        runOnUiThread(() -> {
            Intent i = new Intent(Intent.ACTION_CREATE_DOCUMENT);
            i.addCategory(Intent.CATEGORY_OPENABLE);
            i.setType(mimeType == null || mimeType.isEmpty() ? "application/octet-stream" : mimeType);
            i.putExtra(Intent.EXTRA_TITLE, name);
            i.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION
                    | Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION);
            launch(i, SAVE);
        });
    }

    private void launch(Intent i, int code) {
        try {
            startActivityForResult(i, code);
        } catch (Exception e) {
            answer(new Object[] {"open", new String[0], new byte[0][], "no picker: " + e, new String[0]});
        }
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode != OPEN && requestCode != SAVE) {
            return;
        }
        if (resultCode != RESULT_OK || data == null) {
            answer(new Object[] {"cancel"});
            return;
        }
        if (requestCode == SAVE) {
            Uri uri = data.getData();
            if (uri == null) {
                answer(new Object[] {"cancel"});
            } else {
                keep(uri);
                answer(new Object[] {"save", uri.toString(), displayName(uri)});
            }
            return;
        }
        final ArrayList<Uri> uris = new ArrayList<>();
        ClipData clip = data.getClipData();
        if (clip != null) {
            for (int k = 0; k < clip.getItemCount(); k++) {
                Uri u = clip.getItemAt(k).getUri();
                if (u != null) {
                    uris.add(u);
                }
            }
        } else if (data.getData() != null) {
            uris.add(data.getData());
        }
        // Reading can take a while (a big PSD on Drive): not on the UI thread.
        new Thread(() -> {
            String[] names = new String[uris.size()];
            byte[][] bytes = new byte[uris.size()][];
            String[] kept = new String[uris.size()];
            String error = null;
            int n = 0;
            for (Uri u : uris) {
                try {
                    bytes[n] = read(u);
                    names[n] = displayName(u);
                    kept[n] = keep(u) ? u.toString() : null;
                    n++;
                } catch (Exception e) {
                    error = displayName(u) + ": " + e.getMessage();
                }
            }
            String[] someNames = new String[n];
            byte[][] someBytes = new byte[n][];
            System.arraycopy(names, 0, someNames, 0, n);
            System.arraycopy(bytes, 0, someBytes, 0, n);
            String[] someKept = new String[n];
            System.arraycopy(kept, 0, someKept, 0, n);
            answer(new Object[] {"open", someNames, someBytes, error, someKept});
        }, "photocraft-open").start();
    }

    /** Keep access to {@code uri} across restarts (read and write, else read); false if it can't be kept. */
    private boolean keep(Uri uri) {
        int rw = Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION;
        try {
            getContentResolver().takePersistableUriPermission(uri, rw);
            return true;
        } catch (Exception e) {
            try {
                getContentResolver().takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION);
                return true;
            } catch (Exception ignored) {
                return false;
            }
        }
    }

    /** Read a file reopened from a kept URI (Open Recent). Throws when it can't be read. */
    public byte[] readUri(String uri) throws Exception {
        return read(Uri.parse(uri));
    }

    private byte[] read(Uri uri) throws Exception {
        ContentResolver cr = getContentResolver();
        try (InputStream in = cr.openInputStream(uri)) {
            if (in == null) {
                throw new Exception("can't be read");
            }
            ByteArrayOutputStream out = new ByteArrayOutputStream();
            byte[] buf = new byte[1 << 16];
            long total = 0;
            int r;
            while ((r = in.read(buf)) > 0) {
                total += r;
                if (total > MAX_BYTES) {
                    throw new Exception("larger than 1 GB");
                }
                out.write(buf, 0, r);
            }
            return out.toByteArray();
        }
    }

    private String displayName(Uri uri) {
        try (Cursor c = getContentResolver().query(uri, new String[] {OpenableColumns.DISPLAY_NAME}, null, null, null)) {
            if (c != null && c.moveToFirst()) {
                String name = c.getString(0);
                if (name != null && !name.isEmpty()) {
                    return name;
                }
            }
        } catch (Exception ignored) {
            // Fall back to the URI's last segment.
        }
        String last = uri.getLastPathSegment();
        return last == null || last.isEmpty() ? "Untitled" : last;
    }

    /** Write {@code data} to a content:// URI from {@link #createDocument}; null on success. */
    public String writeUri(String uri, byte[] data) {
        // "wt" truncates what was there; a few providers only know "w".
        String first = write(uri, data, "wt");
        return first == null ? null : write(uri, data, "w") == null ? null : first;
    }

    private String write(String uri, byte[] data, String mode) {
        try (OutputStream out = getContentResolver().openOutputStream(Uri.parse(uri), mode)) {
            if (out == null) {
                return "can't be written";
            }
            out.write(data);
            return null;
        } catch (Exception e) {
            return String.valueOf(e.getMessage());
        }
    }

    // ------------------------------------------------------------------ clipboard and sharing

    /** Write {@code data} to the shared folder as {@code name}; its content:// URI. */
    private Uri shared(String name, byte[] data) throws Exception {
        String safe = name.replaceAll("[^A-Za-z0-9 ._-]", "_");
        if (safe.isEmpty() || safe.startsWith(".")) {
            safe = "image.png";
        }
        File f = new File(ImageProvider.dir(this), safe);
        try (FileOutputStream out = new FileOutputStream(f)) {
            out.write(data);
        }
        return Uri.parse("content://" + ImageProvider.AUTHORITY + "/" + Uri.encode(safe));
    }

    /** Open Android's share sheet with a PNG; null on success, else why not. */
    public String shareImage(byte[] png, String name) {
        try {
            final Uri uri = shared(name, png);
            runOnUiThread(() -> {
                Intent send = new Intent(Intent.ACTION_SEND);
                send.setType("image/png");
                send.putExtra(Intent.EXTRA_STREAM, uri);
                send.setClipData(ClipData.newRawUri(name, uri));
                send.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
                Intent chooser = Intent.createChooser(send, null);
                chooser.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
                try {
                    startActivity(chooser);
                } catch (Exception ignored) {
                    // No app takes images: nothing to share with.
                }
            });
            return null;
        } catch (Exception e) {
            return String.valueOf(e.getMessage());
        }
    }

    /** Put a PNG on Android's clipboard (other apps paste it); null on success. */
    public String setClipboardImage(byte[] png) {
        try {
            final Uri uri = shared("clipboard.png", png);
            runOnUiThread(() -> {
                ClipboardManager cm = (ClipboardManager) getSystemService(Context.CLIPBOARD_SERVICE);
                if (cm != null) {
                    cm.setPrimaryClip(ClipData.newUri(getContentResolver(), "PhotoCraft image", uri));
                }
            });
            return null;
        } catch (Exception e) {
            return String.valueOf(e.getMessage());
        }
    }

    /** The image on Android's clipboard (encoded, e.g. PNG or JPEG), or null when there is none. */
    public byte[] clipboardImage() {
        try {
            ClipboardManager cm = (ClipboardManager) getSystemService(Context.CLIPBOARD_SERVICE);
            if (cm == null || !cm.hasPrimaryClip()) {
                return null;
            }
            ClipData clip = cm.getPrimaryClip();
            if (clip == null || clip.getItemCount() == 0) {
                return null;
            }
            Uri uri = clip.getItemAt(0).getUri();
            if (uri == null) {
                return null;
            }
            String type = getContentResolver().getType(uri);
            ClipDescription d = clip.getDescription();
            boolean image = (type != null && type.startsWith("image/")) || (d != null && d.hasMimeType("image/*"));
            return image ? read(uri) : null;
        } catch (Exception e) {
            return null;
        }
    }
}
