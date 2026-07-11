#pragma once

#include "include/cef_client.h"

namespace WallpaperEngine::WebBrowser::CEF {
// *************************************************************************
//! \brief Provide access to browser-instance-specific callbacks. A single
//! CefClient instance can be shared among any number of browsers.
// *************************************************************************
class BrowserClient : public CefClient, public CefLoadHandler, public CefDisplayHandler {
public:
    explicit BrowserClient (CefRefPtr<CefRenderHandler> ptr);

    [[nodiscard]] CefRefPtr<CefRenderHandler> GetRenderHandler () override;
    [[nodiscard]] CefRefPtr<CefLoadHandler> GetLoadHandler () override;
    [[nodiscard]] CefRefPtr<CefDisplayHandler> GetDisplayHandler () override;

    // CefLoadHandler
    void OnLoadError (
        CefRefPtr<CefBrowser> browser, CefRefPtr<CefFrame> frame, ErrorCode errorCode,
        const CefString& errorText, const CefString& failedUrl
    ) override;
    void OnLoadEnd (CefRefPtr<CefBrowser> browser, CefRefPtr<CefFrame> frame, int httpStatusCode) override;

    // CefDisplayHandler
    bool OnConsoleMessage (
        CefRefPtr<CefBrowser> browser, cef_log_severity_t level, const CefString& message,
        const CefString& source, int line
    ) override;

    CefRefPtr<CefRenderHandler> m_renderHandler = nullptr;

    IMPLEMENT_REFCOUNTING (BrowserClient);
};
} // namespace WallpaperEngine::WebBrowser::CEF
