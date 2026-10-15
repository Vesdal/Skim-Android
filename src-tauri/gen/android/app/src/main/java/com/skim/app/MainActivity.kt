package com.skim.app

import android.os.Bundle
import android.webkit.WebView
import androidx.activity.OnBackPressedCallback
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  private var appView: WebView? = null

  override fun onWebViewCreate(webView: WebView) {
    appView = webView
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    // In-app back navigation first (drawer, dialogs, reader → list), system
    // back (WebView history, then exit) only when the UI consumed nothing.
    // Registered after super.onCreate so it runs before wry's own callback.
    onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
      override fun handleOnBackPressed() {
        val view = appView
        if (view == null) {
          isEnabled = false
          onBackPressedDispatcher.onBackPressed()
          isEnabled = true
          return
        }
        view.evaluateJavascript(
          "(typeof window.__skimGoBack==='function')?window.__skimGoBack():false",
        ) { result ->
          if (result == "true") return@evaluateJavascript
          isEnabled = false
          onBackPressedDispatcher.onBackPressed()
          isEnabled = true
        }
      }
    })
  }
}
