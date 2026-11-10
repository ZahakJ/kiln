// SPDX-License-Identifier: AGPL-3.0-or-later
package io.github.zahakj.kiln

import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.graphics.Color
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.CancellationSignal
import android.os.ParcelFileDescriptor
import android.print.PageRange
import android.print.PrintAttributes
import android.print.PrintDocumentAdapter
import android.print.PrintDocumentInfo
import android.print.PrintManager
import android.provider.DocumentsContract
import android.provider.OpenableColumns
import android.view.View
import android.webkit.WebView
import androidx.activity.result.ActivityResult
import androidx.core.content.FileProvider
import androidx.core.view.WindowCompat
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Channel
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream

@InvokeArg
class OpenDocumentArgs {
    var mimeTypes: Array<String> = arrayOf()
}

@InvokeArg
class CreateDocumentArgs {
    var name: String = ""
    var mimeType: String = "application/octet-stream"
}

@InvokeArg
class UriArgs {
    var uri: String = ""
}

@InvokeArg
class ClipboardArgs {
    var text: String = ""
    var html: String? = null
}

@InvokeArg
class PrintArgs {
    var path: String = ""
    var jobName: String = ""
}

@InvokeArg
class ShareArgs {
    var target: String = ""
    var mimeType: String = "application/octet-stream"
    var name: String = ""
    var title: String = ""
}

@InvokeArg
class ViewArgs {
    var path: String = ""
    var mimeType: String = "application/octet-stream"
}

@InvokeArg
class BarsArgs {
    var color: String = "#F6EFE6"
    var light: Boolean = true
}

@InvokeArg
class WatchIntentsArgs {
    lateinit var channel: Channel
}

/**
 * Kiln's hooks into Android, called by the Rust shell (src/android): the storage access framework
 * (Open and Save As with lasting access to the file), the size and date of a document behind a
 * content URI, the clipboard with its HTML, printing a PDF through PrintManager, sending a file
 * (ACTION_SEND), opening a file in its app, the system bars' colour, and the documents other apps
 * hand to Kiln (VIEW, EDIT, SEND).
 */
@TauriPlugin
class KilnPlugin(private val activity: Activity) : Plugin(activity) {
    private var intents: Channel? = null
    private val pending = mutableListOf<JSObject>()

    override fun load(webView: WebView) {
        queue(activity.intent)
    }

    override fun onNewIntent(intent: Intent) {
        queue(intent)
    }

    /** The documents handed to Kiln, now and from now on, go to `channel`. */
    @Command
    fun watchIntents(invoke: Invoke) {
        val args = invoke.parseArgs(WatchIntentsArgs::class.java)
        synchronized(pending) {
            intents = args.channel
            for (doc in pending) args.channel.send(doc)
            pending.clear()
        }
        invoke.resolve()
    }

    private fun queue(intent: Intent?) {
        if (intent == null) return
        val uris = mutableListOf<Uri>()
        when (intent.action) {
            Intent.ACTION_VIEW, Intent.ACTION_EDIT -> intent.data?.let { uris.add(it) }
            Intent.ACTION_SEND -> streamOf(intent)?.let { uris.add(it) }
            else -> return
        }
        val writable = intent.action == Intent.ACTION_EDIT ||
            (intent.flags and Intent.FLAG_GRANT_WRITE_URI_PERMISSION) != 0
        for (uri in uris) {
            val doc = JSObject()
            doc.put("uri", uri.toString())
            doc.put("name", displayName(uri) ?: uri.lastPathSegment ?: "")
            doc.put("mimeType", intent.type ?: activity.contentResolver.getType(uri) ?: "")
            doc.put("writable", keep(uri, intent.flags) && writable)
            synchronized(pending) {
                val channel = intents
                if (channel != null) channel.send(doc) else pending.add(doc)
            }
        }
        // Handled once: a recreated activity does not open the same documents again.
        activity.intent = Intent(Intent.ACTION_MAIN)
    }

    @Suppress("DEPRECATION")
    private fun streamOf(intent: Intent): Uri? =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java)
        } else {
            intent.getParcelableExtra(Intent.EXTRA_STREAM) as? Uri
        }

    /**
     * Keeps access to a document after a restart (Recent files) where the provider allows it, and
     * tells whether Kiln may write to it.
     */
    private fun keep(uri: Uri, flags: Int): Boolean {
        val resolver = activity.contentResolver
        val write = (flags and Intent.FLAG_GRANT_WRITE_URI_PERMISSION) != 0
        if (write) {
            try {
                resolver.takePersistableUriPermission(
                    uri,
                    Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION
                )
                return true
            } catch (e: SecurityException) {
                // Not a document the provider lets Kiln keep: access lasts while Kiln runs.
            }
        }
        try {
            resolver.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION)
        } catch (e: SecurityException) {
            // As above.
        }
        return write
    }

    private fun displayName(uri: Uri): String? = try {
        activity.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
            if (c.moveToFirst() && !c.isNull(0)) c.getString(0) else null
        }
    } catch (e: Exception) {
        null
    }

    // ----- Open and Save As --------------------------------------------------------------------

    @Command
    fun openDocument(invoke: Invoke) {
        val args = invoke.parseArgs(OpenDocumentArgs::class.java)
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = "*/*"
            if (args.mimeTypes.isNotEmpty()) putExtra(Intent.EXTRA_MIME_TYPES, args.mimeTypes)
            addFlags(
                Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION or
                    Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION
            )
        }
        startActivityForResult(invoke, intent, "documentChosen")
    }

    @Command
    fun createDocument(invoke: Invoke) {
        val args = invoke.parseArgs(CreateDocumentArgs::class.java)
        val intent = Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = args.mimeType
            putExtra(Intent.EXTRA_TITLE, args.name)
            addFlags(
                Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION or
                    Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION
            )
        }
        startActivityForResult(invoke, intent, "documentChosen")
    }

    /** The document picked or created: its URI and name; nothing when the person cancelled. */
    @ActivityCallback
    fun documentChosen(invoke: Invoke, result: ActivityResult) {
        val data = result.data
        val uri = data?.data
        val out = JSObject()
        if (result.resultCode == Activity.RESULT_OK && uri != null) {
            keep(uri, data.flags)
            out.put("uri", uri.toString())
            out.put("name", displayName(uri) ?: uri.lastPathSegment ?: "")
        }
        invoke.resolve(out)
    }

    /** The size and modification time of a document behind a content URI. */
    @Command
    fun stat(invoke: Invoke) {
        val args = invoke.parseArgs(UriArgs::class.java)
        val out = JSObject()
        try {
            val uri = Uri.parse(args.uri)
            val columns = arrayOf(OpenableColumns.SIZE, DocumentsContract.Document.COLUMN_LAST_MODIFIED)
            activity.contentResolver.query(uri, columns, null, null, null)?.use { c ->
                if (c.moveToFirst()) {
                    out.put("exists", true)
                    if (!c.isNull(0)) out.put("size", c.getLong(0))
                    val modified = c.getColumnIndex(DocumentsContract.Document.COLUMN_LAST_MODIFIED)
                    if (modified >= 0 && !c.isNull(modified)) out.put("modified", c.getLong(modified))
                }
            }
        } catch (e: java.io.FileNotFoundException) {
            out.put("exists", false)
        } catch (e: SecurityException) {
            invoke.reject("denied")
            return
        } catch (e: Exception) {
            // A provider that cannot answer: Kiln does without the stamp.
        }
        invoke.resolve(out)
    }

    // ----- Clipboard ---------------------------------------------------------------------------

    @Command
    fun clipboardRead(invoke: Invoke) {
        val clipboard = activity.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        val out = JSObject()
        val clip = clipboard.primaryClip
        if (clip != null && clip.itemCount > 0) {
            val item = clip.getItemAt(0)
            out.put("text", item.coerceToText(activity)?.toString() ?: "")
            item.htmlText?.let { out.put("html", it) }
        } else {
            out.put("text", "")
        }
        invoke.resolve(out)
    }

    @Command
    fun clipboardWrite(invoke: Invoke) {
        val args = invoke.parseArgs(ClipboardArgs::class.java)
        val clipboard = activity.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        val html = args.html
        val clip = if (html != null) {
            ClipData.newHtmlText("Kiln", args.text, html)
        } else {
            ClipData.newPlainText("Kiln", args.text)
        }
        clipboard.setPrimaryClip(clip)
        invoke.resolve()
    }

    // ----- Print, send, open -------------------------------------------------------------------

    /** Prints a PDF through the system's print dialog, named after the document. */
    @Command
    fun print(invoke: Invoke) {
        val args = invoke.parseArgs(PrintArgs::class.java)
        val file = File(args.path)
        if (!file.isFile) {
            invoke.reject("missing")
            return
        }
        val printManager = activity.getSystemService(Context.PRINT_SERVICE) as PrintManager
        printManager.print(args.jobName, PdfDocumentAdapter(file, args.jobName), PrintAttributes.Builder().build())
        invoke.resolve()
    }

    /** Sends a document (a content URI, or a file in Kiln's cache) to the app the person picks. */
    @Command
    fun share(invoke: Invoke) {
        val args = invoke.parseArgs(ShareArgs::class.java)
        val uri = try {
            uriFor(args.target)
        } catch (e: IllegalArgumentException) {
            invoke.reject("missing")
            return
        }
        val send = Intent(Intent.ACTION_SEND).apply {
            type = args.mimeType
            putExtra(Intent.EXTRA_STREAM, uri)
            putExtra(Intent.EXTRA_SUBJECT, args.name)
            putExtra(Intent.EXTRA_TITLE, args.name)
            clipData = ClipData.newRawUri(args.name, uri)
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        try {
            activity.startActivity(Intent.createChooser(send, args.title))
            invoke.resolve()
        } catch (e: ActivityNotFoundException) {
            invoke.reject("noApp")
        }
    }

    /** Opens a file from Kiln's cache in the app that shows its type. */
    @Command
    fun view(invoke: Invoke) {
        val args = invoke.parseArgs(ViewArgs::class.java)
        val uri = try {
            uriFor(args.path)
        } catch (e: IllegalArgumentException) {
            invoke.reject("missing")
            return
        }
        val intent = Intent(Intent.ACTION_VIEW).apply {
            setDataAndType(uri, args.mimeType)
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        try {
            activity.startActivity(intent)
            invoke.resolve()
        } catch (e: ActivityNotFoundException) {
            invoke.reject("noApp")
        }
    }

    private fun uriFor(target: String): Uri =
        if (target.startsWith("content://")) {
            Uri.parse(target)
        } else {
            FileProvider.getUriForFile(activity, activity.packageName + ".fileprovider", File(target))
        }

    // ----- Window ------------------------------------------------------------------------------

    /** The colour behind the status and navigation bars (the page's title bar) and their icons. */
    @Command
    fun setBars(invoke: Invoke) {
        val args = invoke.parseArgs(BarsArgs::class.java)
        val color = try {
            Color.parseColor(args.color)
        } catch (e: IllegalArgumentException) {
            invoke.reject("color")
            return
        }
        activity.runOnUiThread {
            val window = activity.window
            window.decorView.setBackgroundColor(color)
            activity.findViewById<View>(android.R.id.content)?.setBackgroundColor(color)
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.VANILLA_ICE_CREAM) {
                @Suppress("DEPRECATION")
                window.statusBarColor = color
                @Suppress("DEPRECATION")
                window.navigationBarColor = color
            }
            val controller = WindowCompat.getInsetsController(window, window.decorView)
            controller.isAppearanceLightStatusBars = args.light
            controller.isAppearanceLightNavigationBars = args.light
        }
        invoke.resolve()
    }

    /** Back on Kiln Home: Kiln goes to the background, as the launcher's apps do. */
    @Command
    fun moveTaskToBack(invoke: Invoke) {
        activity.runOnUiThread { activity.moveTaskToBack(true) }
        invoke.resolve()
    }
}

/** Hands a finished PDF to the print framework as it is. */
private class PdfDocumentAdapter(private val file: File, private val name: String) : PrintDocumentAdapter() {
    override fun onLayout(
        oldAttributes: PrintAttributes?,
        newAttributes: PrintAttributes,
        cancellationSignal: CancellationSignal?,
        callback: LayoutResultCallback,
        extras: Bundle?
    ) {
        if (cancellationSignal?.isCanceled == true) {
            callback.onLayoutCancelled()
            return
        }
        val info = PrintDocumentInfo.Builder(if (name.endsWith(".pdf")) name else "$name.pdf")
            .setContentType(PrintDocumentInfo.CONTENT_TYPE_DOCUMENT)
            .build()
        callback.onLayoutFinished(info, oldAttributes != newAttributes)
    }

    override fun onWrite(
        pages: Array<out PageRange>,
        destination: ParcelFileDescriptor,
        cancellationSignal: CancellationSignal?,
        callback: WriteResultCallback
    ) {
        try {
            FileInputStream(file).use { input ->
                FileOutputStream(destination.fileDescriptor).use { output -> input.copyTo(output) }
            }
            if (cancellationSignal?.isCanceled == true) {
                callback.onWriteCancelled()
            } else {
                callback.onWriteFinished(arrayOf(PageRange.ALL_PAGES))
            }
        } catch (e: Exception) {
            callback.onWriteFailed(e.message)
        }
    }
}
