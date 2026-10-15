package io.github.photocraft.unofficial;

import android.app.NativeActivity;
import android.content.ClipData;
import android.content.ContentResolver;
import android.content.Intent;
import android.database.Cursor;
import android.net.Uri;
import android.os.Build;
import android.provider.MediaStore;
import android.provider.OpenableColumns;

import java.io.ByteArrayOutputStream;
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
     * {"open", String[] names, byte[][] data, String error-or-null}, {"save", uri, name} or
     * {"cancel"}.
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
            launch(i, SAVE);
        });
    }

    private void launch(Intent i, int code) {
        try {
            startActivityForResult(i, code);
        } catch (Exception e) {
            answer(new Object[] {"open", new String[0], new byte[0][], "no picker: " + e});
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
            String error = null;
            int n = 0;
            for (Uri u : uris) {
                try {
                    bytes[n] = read(u);
                    names[n] = displayName(u);
                    n++;
                } catch (Exception e) {
                    error = displayName(u) + ": " + e.getMessage();
                }
            }
            String[] someNames = new String[n];
            byte[][] someBytes = new byte[n][];
            System.arraycopy(names, 0, someNames, 0, n);
            System.arraycopy(bytes, 0, someBytes, 0, n);
            answer(new Object[] {"open", someNames, someBytes, error});
        }, "photocraft-open").start();
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
}
