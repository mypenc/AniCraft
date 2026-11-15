package ai.storyteller.anicraft

/**
 * Kotlin -> Rust. The symbols are `Java_ai_storyteller_anicraft_NativeBridge_*` in
 * `src/android_glue.rs`; keep the package, object and method names in step.
 */
object NativeBridge {
    /** The answer to a picker Rust asked for: a path in the app's folder, or null if cancelled. */
    @JvmStatic
    external fun onDialogAnswer(id: Long, path: String?)

    /** A document opened from another app ("Open with"): a path in the app's folder. */
    @JvmStatic
    external fun onOpenIntent(path: String)
}
