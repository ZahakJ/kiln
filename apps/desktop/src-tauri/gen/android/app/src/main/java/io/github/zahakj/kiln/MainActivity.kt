// SPDX-License-Identifier: AGPL-3.0-or-later
package io.github.zahakj.kiln

import android.os.Bundle
import android.system.ErrnoException
import android.system.Os
import android.view.KeyCharacterMap
import android.view.KeyEvent
import android.view.View
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

/**
 * Kiln's activity. The page draws edge to edge between the system bars and above the keyboard
 * (their insets pad the content, whose colour the page sets from its theme); Back is Kiln's own
 * (sheets and dialogs close first, then the document: the page listens to the app plugin's
 * back-button event); a hardware keyboard's shortcuts reach the page as on the desktop.
 */
class MainActivity : TauriActivity() {
    private var webView: WebView? = null

    // Back goes to the page, not to the WebView's history (src/android, apps/web/src/desktop.ts).
    override val handleBackNavigation: Boolean = false

    override fun onCreate(savedInstanceState: Bundle?) {
        // The core writes the PDFs it prints and the copies it sends to the temporary folder: the
        // app's cache, which FileProvider shares (res/xml/file_paths.xml).
        try {
            Os.setenv("TMPDIR", cacheDir.absolutePath, true)
        } catch (e: ErrnoException) {
            // Printing then reports that the PDF could not be written.
        }
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        val content = findViewById<View>(android.R.id.content)
        ViewCompat.setOnApplyWindowInsetsListener(content) { view, insets ->
            val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
            val keyboard = insets.getInsets(WindowInsetsCompat.Type.ime())
            view.setPadding(bars.left, bars.top, bars.right, maxOf(bars.bottom, keyboard.bottom))
            WindowInsetsCompat.CONSUMED
        }
    }

    override fun onWebViewCreate(webView: WebView) {
        this.webView = webView
    }

    /**
     * Shortcuts from a hardware keyboard (Ctrl, Alt or Meta with a key, the function keys, Escape)
     * go to the page first, so the desktop's shortcut table works; what the page leaves goes on
     * to Android.
     */
    override fun dispatchKeyEvent(event: KeyEvent): Boolean {
        val web = webView
        if (web != null && event.deviceId != KeyCharacterMap.VIRTUAL_KEYBOARD && isShortcut(event)) {
            if (!web.hasFocus()) web.requestFocus()
            if (web.dispatchKeyEvent(event)) return true
        }
        return super.dispatchKeyEvent(event)
    }

    private fun isShortcut(event: KeyEvent): Boolean {
        val modified = event.isCtrlPressed || event.isAltPressed || event.isMetaPressed
        val key = event.keyCode
        val function = key in KeyEvent.KEYCODE_F1..KeyEvent.KEYCODE_F12
        return (modified && !KeyEvent.isModifierKey(key)) || function || key == KeyEvent.KEYCODE_ESCAPE
    }
}
