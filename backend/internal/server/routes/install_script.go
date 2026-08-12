package routes

import (
	"fmt"
	"net/http"
	"regexp"
	"strings"

	"github.com/gin-gonic/gin"
)

// installKeyPattern restricts the API key that gets embedded into the generated
// shell / PowerShell script to a safe character set. The key is injected into a
// script that users pipe straight into bash/pwsh, so anything outside this set
// is rejected to prevent shell injection.
var installKeyPattern = regexp.MustCompile(`^[A-Za-z0-9._-]{8,256}$`)

// installHostPattern validates the request host (hostname[:port]) before it is
// used to build ANTHROPIC_BASE_URL inside the script.
var installHostPattern = regexp.MustCompile(`^[A-Za-z0-9.-]+(:[0-9]{1,5})?$`)

// RegisterInstallScriptRoutes registers the public one-command Claude Code
// installer endpoints:
//
//	GET /install.sh   -> POSIX shell installer   (curl … | bash)
//	GET /install.ps1  -> PowerShell installer     (irm … | iex)
//
// Both accept ?key=<api-key> and configure Claude Code to route through this
// gateway. Passing the uninstall flag (bash -s -- --uninstall, or the script's
// -Uninstall switch) removes the configuration again.
func RegisterInstallScriptRoutes(r *gin.Engine) {
	r.GET("/install.sh", serveInstallShellScript)
	r.GET("/install.ps1", serveInstallPowerShellScript)
}

// resolveInstallBaseURL derives the gateway base URL from the incoming request.
func resolveInstallBaseURL(c *gin.Context) (string, bool) {
	host := strings.TrimSpace(c.Request.Host)
	if host == "" {
		host = strings.TrimSpace(c.GetHeader("X-Forwarded-Host"))
	}
	if !installHostPattern.MatchString(host) {
		return "", false
	}

	scheme := "https"
	if proto := strings.TrimSpace(c.GetHeader("X-Forwarded-Proto")); proto != "" {
		scheme = strings.ToLower(strings.Split(proto, ",")[0])
	} else if c.Request.TLS == nil {
		// No TLS terminated at this hop and no proxy hint: fall back to http so
		// local/dev setups keep working.
		scheme = "http"
	}
	if scheme != "http" && scheme != "https" {
		scheme = "https"
	}

	return fmt.Sprintf("%s://%s", scheme, host), true
}

func serveInstallShellScript(c *gin.Context) {
	key := strings.TrimSpace(c.Query("key"))
	baseURL, ok := resolveInstallBaseURL(c)
	if !ok {
		c.String(http.StatusBadRequest, "# invalid request host\n")
		return
	}
	if key != "" && !installKeyPattern.MatchString(key) {
		c.String(http.StatusBadRequest, "# invalid key parameter\n")
		return
	}

	script := buildShellInstallScript(baseURL, key)
	c.Header("Cache-Control", "no-store")
	c.Data(http.StatusOK, "text/x-shellscript; charset=utf-8", []byte(script))
}

func serveInstallPowerShellScript(c *gin.Context) {
	key := strings.TrimSpace(c.Query("key"))
	baseURL, ok := resolveInstallBaseURL(c)
	if !ok {
		c.String(http.StatusBadRequest, "# invalid request host\n")
		return
	}
	if key != "" && !installKeyPattern.MatchString(key) {
		c.String(http.StatusBadRequest, "# invalid key parameter\n")
		return
	}

	script := buildPowerShellInstallScript(baseURL, key)
	c.Header("Cache-Control", "no-store")
	c.Data(http.StatusOK, "text/plain; charset=utf-8", []byte(script))
}

// buildShellInstallScript renders the POSIX installer. baseURL and key are
// pre-validated, so they can be embedded inside single quotes safely.
func buildShellInstallScript(baseURL, key string) string {
	return fmt.Sprintf(installShellTemplate, baseURL, key)
}

// buildPowerShellInstallScript renders the PowerShell installer.
func buildPowerShellInstallScript(baseURL, key string) string {
	return fmt.Sprintf(installPowerShellTemplate, baseURL, key)
}

// installShellTemplate is the POSIX shell installer. %[1]s = base URL, %[2]s = key.
const installShellTemplate = `#!/usr/bin/env bash
# Claude Code configuration installer for Mishra Miron API.
# Install:   curl -fsSL "%[1]s/install.sh?key=YOUR_KEY" | bash
# Uninstall: curl -fsSL "%[1]s/install.sh?key=YOUR_KEY" | bash -s -- --uninstall
set -euo pipefail

BASE_URL='%[1]s'
API_KEY='%[2]s'
MARKER_BEGIN='# >>> mishra-miron-api (Claude Code) >>>'
MARKER_END='# <<< mishra-miron-api (Claude Code) <<<'
SETTINGS_DIR="$HOME/.claude"
SETTINGS_FILE="$SETTINGS_DIR/settings.json"

ACTION='install'
if [ "${1:-}" = '--uninstall' ] || [ "${1:-}" = 'uninstall' ]; then
  ACTION='uninstall'
fi

info()  { printf '\033[0;36m%%s\033[0m\n' "$1"; }
ok()    { printf '\033[0;32m%%s\033[0m\n' "$1"; }
warn()  { printf '\033[0;33m%%s\033[0m\n' "$1"; }

rc_files() {
  local files=()
  [ -n "${ZSH_VERSION:-}" ] && files+=("$HOME/.zshrc")
  [ -f "$HOME/.zshrc" ] && files+=("$HOME/.zshrc")
  [ -f "$HOME/.bashrc" ] && files+=("$HOME/.bashrc")
  [ -f "$HOME/.bash_profile" ] && files+=("$HOME/.bash_profile")
  if [ ${#files[@]} -eq 0 ]; then
    files+=("$HOME/.profile")
  fi
  printf '%%s\n' "${files[@]}" | awk '!seen[$0]++'
}

strip_block() {
  # $1 = file. Removes any existing managed block (portable, no in-place sed).
  local file="$1"
  [ -f "$file" ] || return 0
  local tmp
  tmp="$(mktemp)"
  awk -v b="$MARKER_BEGIN" -v e="$MARKER_END" '
    $0==b {skip=1; next}
    $0==e {skip=0; next}
    skip!=1 {print}
  ' "$file" > "$tmp"
  # Trim trailing blank lines left behind.
  awk 'NF{blank=0} !NF{blank++} {lines[NR]=$0} END{for(i=1;i<=NR-blank;i++) print lines[i]}' "$tmp" > "$file"
  rm -f "$tmp"
}

write_block() {
  local file="$1"
  strip_block "$file"
  {
    printf '\n%%s\n' "$MARKER_BEGIN"
    printf 'export ANTHROPIC_BASE_URL="%%s"\n' "$BASE_URL"
    printf 'export ANTHROPIC_AUTH_TOKEN="%%s"\n' "$API_KEY"
    printf 'export CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1\n'
    printf 'export CLAUDE_CODE_ATTRIBUTION_HEADER=0\n'
    printf 'unset ANTHROPIC_API_KEY 2>/dev/null || true\n'
    printf '%%s\n' "$MARKER_END"
  } >> "$file"
}

update_settings() {
  # Merge env block into ~/.claude/settings.json using python3, then node, else create.
  mkdir -p "$SETTINGS_DIR"
  if command -v python3 >/dev/null 2>&1; then
    BASE_URL="$BASE_URL" API_KEY="$API_KEY" SETTINGS_FILE="$SETTINGS_FILE" python3 - <<'PY'
import json, os
path = os.environ['SETTINGS_FILE']
try:
    with open(path) as f:
        data = json.load(f)
    if not isinstance(data, dict):
        data = {}
except Exception:
    data = {}
env = data.get('env')
if not isinstance(env, dict):
    env = {}
env.update({
    'ANTHROPIC_BASE_URL': os.environ['BASE_URL'],
    'ANTHROPIC_AUTH_TOKEN': os.environ['API_KEY'],
    'CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC': '1',
    'CLAUDE_CODE_ATTRIBUTION_HEADER': '0',
})
env.pop('ANTHROPIC_API_KEY', None)
data['env'] = env
data.setdefault('$schema', 'https://json.schemastore.org/claude-code-settings.json')
with open(path, 'w') as f:
    json.dump(data, f, indent=2)
    f.write('\n')
PY
  elif [ ! -f "$SETTINGS_FILE" ]; then
    cat > "$SETTINGS_FILE" <<JSON
{
  "\$schema": "https://json.schemastore.org/claude-code-settings.json",
  "env": {
    "ANTHROPIC_BASE_URL": "$BASE_URL",
    "ANTHROPIC_AUTH_TOKEN": "$API_KEY",
    "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1",
    "CLAUDE_CODE_ATTRIBUTION_HEADER": "0"
  }
}
JSON
  else
    warn "python3 not found; left $SETTINGS_FILE untouched. Set env vars manually if needed."
  fi
}

clean_settings() {
  [ -f "$SETTINGS_FILE" ] || return 0
  if command -v python3 >/dev/null 2>&1; then
    SETTINGS_FILE="$SETTINGS_FILE" python3 - <<'PY'
import json, os
path = os.environ['SETTINGS_FILE']
try:
    with open(path) as f:
        data = json.load(f)
except Exception:
    raise SystemExit(0)
if isinstance(data, dict) and isinstance(data.get('env'), dict):
    for k in ('ANTHROPIC_BASE_URL','ANTHROPIC_AUTH_TOKEN','CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC','CLAUDE_CODE_ATTRIBUTION_HEADER'):
        data['env'].pop(k, None)
    if not data['env']:
        data.pop('env', None)
    with open(path, 'w') as f:
        json.dump(data, f, indent=2)
        f.write('\n')
PY
  fi
}

if [ "$ACTION" = 'uninstall' ]; then
  info "Removing Mishra Miron API Claude Code configuration…"
  while IFS= read -r file; do
    [ -n "$file" ] && strip_block "$file"
  done <<< "$(rc_files)"
  clean_settings
  ok "Removed. Restart your terminal (or run: unset ANTHROPIC_BASE_URL ANTHROPIC_AUTH_TOKEN)."
  exit 0
fi

if [ -z "$API_KEY" ]; then
  warn "No API key supplied. Re-run with ?key=YOUR_KEY in the URL."
  exit 1
fi

info "Configuring Claude Code for Mishra Miron API…"
while IFS= read -r file; do
  [ -n "$file" ] && write_block "$file"
done <<< "$(rc_files)"
update_settings
ok "Done. Claude Code will now route through: $BASE_URL"
info "Next:  1) reload your shell (e.g. source ~/.zshrc)   2) run: claude"
`

// installPowerShellTemplate is the PowerShell installer. %[1]s = base URL, %[2]s = key.
const installPowerShellTemplate = `# Claude Code configuration installer for Mishra Miron API (Windows).
# Install:   irm "%[1]s/install.ps1?key=YOUR_KEY" | iex
# Uninstall: & ([scriptblock]::Create((irm "%[1]s/install.ps1?key=YOUR_KEY"))) -Uninstall
param([switch]$Uninstall)

$ErrorActionPreference = 'Stop'
$BaseUrl  = '%[1]s'
$ApiKey   = '%[2]s'
$SettingsDir  = Join-Path $env:USERPROFILE '.claude'
$SettingsFile = Join-Path $SettingsDir 'settings.json'

$vars = @{
  ANTHROPIC_BASE_URL                        = $BaseUrl
  ANTHROPIC_AUTH_TOKEN                      = $ApiKey
  CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC  = '1'
  CLAUDE_CODE_ATTRIBUTION_HEADER            = '0'
}

function Write-FreshSettings {
  $obj = [ordered]@{
    '$schema' = 'https://json.schemastore.org/claude-code-settings.json'
    env       = $vars
  }
  ($obj | ConvertTo-Json -Depth 10) | Set-Content -Path $SettingsFile -Encoding utf8
}

function Write-Settings {
  if (-not (Test-Path $SettingsDir)) { New-Item -ItemType Directory -Path $SettingsDir -Force | Out-Null }
  # PowerShell 7+ can safely merge into an existing settings.json (-AsHashtable).
  # Windows PowerShell 5.1 lacks -AsHashtable; to avoid clobbering an existing
  # file we only create it when missing there and otherwise rely on the
  # persisted user environment variables set above.
  if ($PSVersionTable.PSVersion.Major -ge 6) {
    $data = @{}
    if (Test-Path $SettingsFile) {
      try { $data = Get-Content $SettingsFile -Raw | ConvertFrom-Json -AsHashtable } catch { $data = @{} }
      if ($null -eq $data) { $data = @{} }
    }
    if (-not $data.ContainsKey('env') -or $null -eq $data['env']) { $data['env'] = @{} }
    $env2 = @{}
    foreach ($k in $data['env'].Keys) { $env2[$k] = $data['env'][$k] }
    foreach ($k in $vars.Keys) { $env2[$k] = $vars[$k] }
    $env2.Remove('ANTHROPIC_API_KEY') | Out-Null
    $data['env'] = $env2
    if (-not $data.ContainsKey('$schema')) { $data['$schema'] = 'https://json.schemastore.org/claude-code-settings.json' }
    ($data | ConvertTo-Json -Depth 10) | Set-Content -Path $SettingsFile -Encoding utf8
  }
  elseif (-not (Test-Path $SettingsFile)) {
    Write-FreshSettings
  }
  else {
    Write-Host "Note: kept existing $SettingsFile untouched (PowerShell 5.1). User environment variables are set." -ForegroundColor Yellow
  }
}

function Clear-Settings {
  if (-not (Test-Path $SettingsFile)) { return }
  if ($PSVersionTable.PSVersion.Major -lt 6) { return }
  try { $data = Get-Content $SettingsFile -Raw | ConvertFrom-Json -AsHashtable } catch { return }
  if ($null -ne $data -and $data.ContainsKey('env') -and $null -ne $data['env']) {
    foreach ($k in @($vars.Keys)) { $data['env'].Remove($k) | Out-Null }
    if ($data['env'].Count -eq 0) { $data.Remove('env') | Out-Null }
    ($data | ConvertTo-Json -Depth 10) | Set-Content -Path $SettingsFile -Encoding utf8
  }
}

if ($Uninstall) {
  Write-Host 'Removing Mishra Miron API Claude Code configuration…' -ForegroundColor Cyan
  foreach ($k in $vars.Keys) { [Environment]::SetEnvironmentVariable($k, $null, 'User') }
  Clear-Settings
  Write-Host 'Removed. Open a new terminal for the change to take effect.' -ForegroundColor Green
  return
}

if ([string]::IsNullOrWhiteSpace($ApiKey)) {
  Write-Host 'No API key supplied. Re-run with ?key=YOUR_KEY in the URL.' -ForegroundColor Yellow
  return
}

Write-Host 'Configuring Claude Code for Mishra Miron API…' -ForegroundColor Cyan
foreach ($k in $vars.Keys) {
  [Environment]::SetEnvironmentVariable($k, $vars[$k], 'User')
  Set-Item -Path ("Env:" + $k) -Value $vars[$k]
}
[Environment]::SetEnvironmentVariable('ANTHROPIC_API_KEY', $null, 'User')
Write-Settings
Write-Host "Done. Claude Code will now route through: $BaseUrl" -ForegroundColor Green
Write-Host 'Next: open a new terminal, then run: claude' -ForegroundColor Cyan
`
