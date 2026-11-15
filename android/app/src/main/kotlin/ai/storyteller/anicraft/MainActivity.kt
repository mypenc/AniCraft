package ai.storyteller.anicraft

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Bundle
import android.provider.OpenableColumns
import android.os.Build
import android.util.Log
import android.view.WindowManager
import androidx.core.view.WindowInsetsControllerCompat
import android.view.Gravity
import android.view.HapticFeedbackConstants
import android.view.View
import android.view.ViewGroup
import android.view.inputmethod.InputMethodManager
import android.widget.FrameLayout
import android.widget.ProgressBar
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import com.google.androidgamesdk.GameActivity
import java.io.File
import java.util.concurrent.Executors

/**
 * Hosts the Rust app. Rust calls the `@Suppress("unused")` methods below over JNI (see
 * `src/android_glue.rs`); results go back through [NativeBridge].
 *
 * Android gives apps `content://` URIs, but the engine reads and writes plain paths. So:
 *  - **Open / import**: the picked file is copied into `filesDir/imports/...` and its path is
 *    handed to Rust (a picked document is mirrored back to the original after each save);
 *  - **Save / Export**: the user first names the destination here; Rust then writes to
 *    `getExternalFilesDir()/exports/<name>` and [Exports] mirrors every change to that document.
 */
class MainActivity : GameActivity() {
    private val io = Executors.newSingleThreadExecutor()
    private lateinit var exports: Exports
    private var loadingView: View? = null
    private var loadingBar: ProgressBar? = null
    @Volatile private var fullscreen = true

    private class Request(val id: Long, val arg: String)

    // One system dialog at a time; the rest wait their turn.
    private val queue = ArrayDeque<Pair<Boolean, Request>>() // (isSave, request)
    private var active: Pair<Boolean, Request>? = null

    /** Open with write access kept across restarts, so Save can write back to the picked file. */
    private class OpenWritable : ActivityResultContracts.OpenDocument() {
        override fun createIntent(context: Context, input: Array<String>): Intent =
            super.createIntent(context, input).addFlags(
                Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION or
                    Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION
            )
    }

    private val openDocument = registerForActivityResult(OpenWritable()) { uri ->
        if (uri != null) runCatching {
            contentResolver.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
        } // read-only providers refuse the write grant; opening still works
        finish(uri) { u, req -> copyIn(u, req) }
    }

    private val createDocument = registerForActivityResult(ActivityResultContracts.CreateDocument("*/*")) { uri ->
        finish(uri) { u, req -> stage(u, req.id) }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        // Android 15 forces edge-to-edge, so the system bars would cover the top of the app (the
        // menu bar). Nothing on the Rust side reads the insets, so pad the content view by them.
        WindowCompat.setDecorFitsSystemWindows(window, false)
        super.onCreate(savedInstanceState)
        val content = findViewById<View>(android.R.id.content)
        ViewCompat.setOnApplyWindowInsetsListener(content) { view, insets ->
            // Full screen: the system bars are hidden, so only the notch / camera cut-out is kept
            // clear; the window colour fills behind it, so there is no gap.
            val bars: Insets = if (fullscreen) insets.getInsets(WindowInsetsCompat.Type.displayCutout())
                else insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
            view.setPadding(bars.left, bars.top, bars.right, bars.bottom)
            WindowInsetsCompat.CONSUMED
        }
        applyFullscreen(true) // Rust corrects this on its first frame if the preference is off
        ViewCompat.requestApplyInsets(content)
        showLoading()
        exports = Exports(applicationContext)
        DebugLog.install(this)
        handleViewIntent(intent)
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus && fullscreen) applyFullscreen(true) // the system shows the bars again after a dialog
    }

    private fun applyFullscreen(on: Boolean) {
        fullscreen = on
        val lp = window.attributes
        lp.layoutInDisplayCutoutMode = when {
            !on -> WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_DEFAULT
            Build.VERSION.SDK_INT >= 30 -> WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS
            else -> WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
        }
        window.attributes = lp
        val controller = WindowCompat.getInsetsController(window, window.decorView)
        if (on) {
            controller.systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            controller.hide(WindowInsetsCompat.Type.systemBars())
        } else {
            controller.show(WindowInsetsCompat.Type.systemBars())
        }
        ViewCompat.requestApplyInsets(findViewById(android.R.id.content))
    }

    /** Help > Full Screen: hide the status / navigation bars and draw into the notch area. */
    @Suppress("unused")
    fun setFullscreen(on: Boolean) = runOnUiThread { applyFullscreen(on) }

    override fun onDestroy() {
        if (isFinishing) DebugLog.markClean(this)
        super.onDestroy()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handleViewIntent(intent)
    }

    // ---- called from Rust --------------------------------------------------------------

    /** File > Open and imports. [purpose] is `document` for File > Open (saves mirror back). */
    @Suppress("unused")
    fun openDialog(id: Long, purpose: String) = runOnUiThread { enqueue(false, Request(id, purpose)) }

    /** Save As / Export: [name] is the suggested file name. */
    @Suppress("unused")
    fun saveDialog(id: Long, name: String) = runOnUiThread { enqueue(true, Request(id, name)) }

    /** Rust: how far startup has got (0 to 100). */
    @Suppress("unused")
    fun setLoadingProgress(percent: Int) = runOnUiThread { loadingBar?.progress = percent.coerceIn(0, 100) }

    /** Rust: the first frame is drawn, so the loading bar goes away. */
    @Suppress("unused")
    fun hideLoading() = runOnUiThread {
        loadingView?.let { (it.parent as? ViewGroup)?.removeView(it) }
        loadingView = null
        loadingBar = null
    }

    /** A plain loading bar on the window colour, shown until AniCraft draws. */
    private fun showLoading() {
        val bar = ProgressBar(this, null, android.R.attr.progressBarStyleHorizontal).apply {
            isIndeterminate = false
            max = 100
            progress = 3
            progressTintList = android.content.res.ColorStateList.valueOf(0xFF33B5E5.toInt())
        }
        loadingBar = bar
        val overlay = FrameLayout(this).apply {
            setBackgroundColor(0xFF1E2327.toInt())
            isClickable = true
            val w = (resources.displayMetrics.density * 240).toInt()
            addView(bar, FrameLayout.LayoutParams(w, ViewGroup.LayoutParams.WRAP_CONTENT, Gravity.CENTER))
        }
        loadingView = overlay
        addContentView(overlay, ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
    }

    /** The long press that opened a context menu. */
    @Suppress("unused")
    fun haptic() = runOnUiThread { window.decorView.performHapticFeedback(HapticFeedbackConstants.LONG_PRESS) }

    /** The bar's KB button: open the keyboard even where the normal request is ignored. */
    @Suppress("unused")
    fun forceShowSoftKeyboard() = runOnUiThread {
        val view = currentFocus ?: window.decorView
        view.requestFocus()
        val inputMethod = getSystemService(INPUT_METHOD_SERVICE) as InputMethodManager
        inputMethod.restartInput(view)
        @Suppress("DEPRECATION")
        var shown = inputMethod.showSoftInput(view, InputMethodManager.SHOW_FORCED)
        if (android.os.Build.VERSION.SDK_INT >= 30) {
            window.insetsController?.show(android.view.WindowInsets.Type.ime())
            shown = true
        }
        if (!shown) Log.w("AniCraft", "Android did not accept the request to show the soft keyboard")
    }

    /** Copy / Cut in a text field: put the text where other apps can paste it. */
    @Suppress("unused")
    fun setClipboardText(text: String) = runOnUiThread {
        val cm = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        cm.setPrimaryClip(ClipData.newPlainText("AniCraft", text))
    }

    // ---- dialogs -----------------------------------------------------------------------

    private fun enqueue(isSave: Boolean, request: Request) {
        queue.addLast(isSave to request)
        next()
    }

    private fun next() {
        if (active != null) return
        val item = queue.removeFirstOrNull() ?: return
        active = item
        val (isSave, request) = item
        if (isSave) createDocument.launch(request.arg) else openDocument.launch(arrayOf("*/*"))
    }

    private fun finish(uri: Uri?, work: (Uri, Request) -> Unit) {
        val request = active?.second
        active = null
        if (request == null) return
        if (uri == null) NativeBridge.onDialogAnswer(request.id, null) else work(uri, request)
        next()
    }

    /** Open: copy the file into the app's folder (off the UI thread) and answer with its path. */
    private fun copyIn(uri: Uri, request: Request) = io.execute {
        val dir = File(filesDir, "imports/${System.currentTimeMillis()}").apply { mkdirs() }
        val path = try {
            val target = File(dir, displayName(uri))
            contentResolver.openInputStream(uri)?.use { input -> target.outputStream().use { input.copyTo(it, 1 shl 20) } }
                ?: error("cannot open $uri")
            // Save writes the app's copy; mirror it back to the picked file when we may write there.
            if (request.arg == "document") mirrorBack(target, uri)
            target.absolutePath
        } catch (t: Throwable) {
            Log.w("AniCraft", "could not read $uri", t)
            null
        }
        NativeBridge.onDialogAnswer(request.id, path)
    }

    /**
     * Save: the file the user named already exists (empty) at [uri]. Rust writes to a staging file
     * named like it (the extension picks the format) and [Exports] mirrors it to [uri] after every
     * save while the app runs.
     */
    private fun stage(uri: Uri, id: Long) = io.execute {
        val dir = File(getExternalFilesDir(null) ?: filesDir, "exports").apply { mkdirs() }
        val staged = File(dir, displayName(uri))
        runCatching { staged.delete() } // never mirror a stale file before the app writes
        exports.watch(staged, uri, true)
        NativeBridge.onDialogAnswer(id, staged.absolutePath)
    }

    /** Keep [uri] in step with the app's copy [file] after every save, if write access was granted. */
    private fun mirrorBack(file: File, uri: Uri) {
        val writable = checkUriPermission(uri, android.os.Process.myPid(), android.os.Process.myUid(), Intent.FLAG_GRANT_WRITE_URI_PERMISSION) ==
            PackageManager.PERMISSION_GRANTED
        if (writable) exports.watch(file, uri, true, alreadySynced = true)
    }

    private fun handleViewIntent(intent: Intent?) {
        val uri = intent?.takeIf { it.action == Intent.ACTION_VIEW }?.data ?: return
        io.execute {
            val dir = File(filesDir, "imports/${System.currentTimeMillis()}").apply { mkdirs() }
            try {
                val target = File(dir, displayName(uri))
                contentResolver.openInputStream(uri)?.use { input -> target.outputStream().use { input.copyTo(it, 1 shl 20) } }
                    ?: return@execute
                mirrorBack(target, uri)
                NativeBridge.onOpenIntent(target.absolutePath)
            } catch (t: Throwable) {
                Log.w("AniCraft", "could not open $uri", t)
            }
        }
    }

    private fun displayName(uri: Uri): String {
        if (uri.scheme == "content") {
            contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
                if (c.moveToFirst()) c.getString(0)?.let { return it.replace('/', '_') }
            }
        }
        return uri.lastPathSegment?.substringAfterLast('/') ?: "document"
    }
}
