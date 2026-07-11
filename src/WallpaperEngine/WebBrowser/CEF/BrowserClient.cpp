#include "BrowserClient.h"
#include <iostream>

using namespace WallpaperEngine::WebBrowser::CEF;

BrowserClient::BrowserClient (CefRefPtr<CefRenderHandler> ptr) : m_renderHandler (std::move (ptr)) { }

CefRefPtr<CefRenderHandler> BrowserClient::GetRenderHandler () { return m_renderHandler; }
CefRefPtr<CefLoadHandler> BrowserClient::GetLoadHandler () { return this; }
CefRefPtr<CefDisplayHandler> BrowserClient::GetDisplayHandler () { return this; }

void BrowserClient::OnLoadError (
    CefRefPtr<CefBrowser> browser, CefRefPtr<CefFrame> frame, ErrorCode errorCode,
    const CefString& errorText, const CefString& failedUrl
) {
    // ERR_ABORTED (-3) is fired for normal navigations that get superseded; ignore it.
    if (errorCode == ERR_ABORTED) {
	return;
    }
    std::cerr << "[LWE-WEB] OnLoadError code=" << errorCode << " text=" << errorText.ToString ()
              << " url=" << failedUrl.ToString () << std::endl;
}

void BrowserClient::OnLoadEnd (CefRefPtr<CefBrowser> browser, CefRefPtr<CefFrame> frame, int httpStatusCode) {
    // only report non-success loads of the main frame
    if (frame->IsMain () && httpStatusCode != 200 && httpStatusCode != 0) {
	std::cerr << "[LWE-WEB] main frame loaded with http=" << httpStatusCode << " url="
	          << frame->GetURL ().ToString () << std::endl;
    }
}

bool BrowserClient::OnConsoleMessage (
    CefRefPtr<CefBrowser> browser, cef_log_severity_t level, const CefString& message, const CefString& source,
    int line
) {
    // only surface actual JS errors, not warnings/logs (keeps output clean)
    if (level >= LOGSEVERITY_ERROR) {
	std::cerr << "[LWE-WEB] JS error: " << message.ToString () << " (" << source.ToString () << ":" << line
	          << ")" << std::endl;
    }
    return false;
}
