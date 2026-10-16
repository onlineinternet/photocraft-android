package io.github.photocraft.unofficial;

import android.content.ContentProvider;
import android.content.ContentValues;
import android.database.Cursor;
import android.database.MatrixCursor;
import android.net.Uri;
import android.os.ParcelFileDescriptor;
import android.provider.OpenableColumns;

import java.io.File;
import java.io.FileNotFoundException;

/**
 * Hands the images PhotoCraft copies or shares to other apps (the clipboard, the share sheet):
 * read-only files in the app's cache, at content://io.github.photocraft.unofficial.images/NAME.
 * Other apps only get to read one through a permission Android grants for that copy or share.
 */
public class ImageProvider extends ContentProvider {
    public static final String AUTHORITY = "io.github.photocraft.unofficial.images";

    /** Where the files live (app cache, so Android may clear it). */
    static File dir(android.content.Context c) {
        File d = new File(c.getCacheDir(), "shared");
        d.mkdirs();
        return d;
    }

    private File file(Uri uri) throws FileNotFoundException {
        String name = uri.getLastPathSegment();
        // Only plain names in our folder: no "..", no subfolders.
        if (name == null || name.isEmpty() || name.contains("/") || name.startsWith(".")) {
            throw new FileNotFoundException(String.valueOf(uri));
        }
        File f = new File(dir(getContext()), name);
        if (!f.isFile()) {
            throw new FileNotFoundException(String.valueOf(uri));
        }
        return f;
    }

    @Override
    public boolean onCreate() {
        return true;
    }

    @Override
    public ParcelFileDescriptor openFile(Uri uri, String mode) throws FileNotFoundException {
        return ParcelFileDescriptor.open(file(uri), ParcelFileDescriptor.MODE_READ_ONLY);
    }

    @Override
    public String getType(Uri uri) {
        String n = String.valueOf(uri.getLastPathSegment()).toLowerCase();
        return n.endsWith(".png") ? "image/png" : n.endsWith(".jpg") ? "image/jpeg" : "application/octet-stream";
    }

    @Override
    public Cursor query(Uri uri, String[] projection, String selection, String[] args, String sort) {
        File f;
        try {
            f = file(uri);
        } catch (FileNotFoundException e) {
            return null;
        }
        MatrixCursor c = new MatrixCursor(new String[] {OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE});
        c.addRow(new Object[] {f.getName(), f.length()});
        return c;
    }

    @Override
    public Uri insert(Uri uri, ContentValues values) {
        return null;
    }

    @Override
    public int delete(Uri uri, String selection, String[] args) {
        return 0;
    }

    @Override
    public int update(Uri uri, ContentValues values, String selection, String[] args) {
        return 0;
    }
}
