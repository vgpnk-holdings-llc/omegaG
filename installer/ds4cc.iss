; DS4CC v2 — Inno Setup installer script
; Build:
;   1. cargo build --release
;   2. Open this file in Inno Setup Compiler (https://jrsoftware.org/isinfo.php)
;   3. Press Compile — output lands in installer\output\DS4CC-Setup.exe

#define MyAppName      "DS4CC"
#define MyAppVersion   "3.2.0"
#define MyAppPublisher "VeigaPunk"
#define MyAppURL       "https://github.com/VeigaPunk/DS4CC"
#define MyAppExe       "ds4cc.exe"
#define WisprURL       "https://ref.wisprflow.ai/vgpnk"
; LordOfMice HID USB filter / buffering overclock (hidusbf) — optional companion
; for DualShock 4 / DualSense polling. Opens the official project after install
; when the task is left checked (opt-out). Not bundled; third-party driver install.
#define LordOfMiceURL  "https://github.com/LordOfMice/hidusbf"

; ── Setup ──────────────────────────────────────────────────────────────
[Setup]
AppId={{F3A2C1D4-8B7E-4F5A-9C6D-2E1B0A3F4C5D}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}

; Install to %LOCALAPPDATA%\DS4CC — no UAC prompt, no "Select install mode" dialog
DefaultDirName={localappdata}\{#MyAppName}
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=commandline

; Output
OutputDir=output
OutputBaseFilename=DS4CC-Setup
SetupIconFile=..\assets\icon.ico

; Appearance
WizardStyle=modern
DisableProgramGroupPage=yes
DisableWelcomePage=no

; Compression
Compression=lzma2/ultra64
SolidCompression=yes
LZMAUseSeparateProcess=yes

; Close running instance before upgrading
CloseApplications=force
CloseApplicationsFilter=*.exe

; Uninstall
UninstallDisplayIcon={app}\{#MyAppExe}
UninstallDisplayName={#MyAppName}

; Version info embedded in the installer .exe
VersionInfoVersion={#MyAppVersion}.0
VersionInfoProductName={#MyAppName}
VersionInfoProductVersion={#MyAppVersion}

; ── Languages ──────────────────────────────────────────────────────────
[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

; ── Tasks (optional features shown as checkboxes) ──────────────────────
[Tasks]
; Auto-start: OFF by default — user must consciously enable it
Name: "autostart";  Description: "Start {#MyAppName} automatically when Windows starts"; \
                    GroupDescription: "Startup:"; Flags: unchecked
; Desktop shortcut: on by default, easy to opt out
Name: "desktopicon"; Description: "Create a desktop shortcut"; \
                     GroupDescription: "Additional icons:"; Flags: checkedonce
; LordOfMice hidusbf: ON by default — uncheck if you do not want the download page
Name: "lordofmice"; Description: "Open LordOfMice hidusbf (USB HID buffering / overclock) after install — recommended for DualSense/DS4 input lag"; \
                    GroupDescription: "Controller input (optional):"; Flags: checkedonce
; Wispr Flow: off by default
Name: "wisprflow";  Description: "Open the Wispr Flow download page after install (required for Speech-to-Text)"; \
                    GroupDescription: "Speech-to-Text:"; Flags: unchecked

; ── Files ──────────────────────────────────────────────────────────────
[Files]
Source: "..\target\x86_64-pc-windows-gnu\release\{#MyAppExe}"; DestDir: "{app}"; Flags: ignoreversion

; ── Icons (Start Menu + optional desktop) ──────────────────────────────
[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExe}"
Name: "{autodesktop}\{#MyAppName}";  Filename: "{app}\{#MyAppExe}"; Tasks: desktopicon

; ── Registry ───────────────────────────────────────────────────────────
[Registry]
; Auto-start entry — only added when the task is checked; removed on uninstall
Root: HKCU; \
  Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; \
  ValueType: string; ValueName: "{#MyAppName}"; \
  ValueData: """{app}\{#MyAppExe}"""; \
  Flags: uninsdeletevalue; Tasks: autostart

; ── Uninstall cleanup ─────────────────────────────────────────────
[UninstallDelete]
; Config directory (%APPDATA%\ds4cc — config.toml lives here)
Type: filesandordirs; Name: "{userappdata}\ds4cc"
; Temp state directory (%TEMP%\DS4CC — agent state files)
Type: filesandordirs; Name: "{%TEMP}\DS4CC"

; ── Post-install actions ───────────────────────────────────────────────
[Run]
; Launch the app when the user clicks "Finish" (optional tick, default on)
Filename: "{app}\{#MyAppExe}"; \
  Description: "Launch {#MyAppName} now"; \
  Flags: nowait postinstall skipifsilent runhidden

; Open LordOfMice hidusbf project — only if the task was left checked (opt-out)
Filename: "{#LordOfMiceURL}"; \
  Description: "Open LordOfMice hidusbf (USB HID buffering / overclock)"; \
  Flags: shellexec postinstall skipifsilent; \
  Tasks: lordofmice

; Open Wispr Flow download page — only if the task was checked
Filename: "{#WisprURL}"; \
  Description: "Open Wispr Flow download page"; \
  Flags: shellexec postinstall skipifsilent; \
  Tasks: wisprflow

; ── Installer logic (Pascal) ───────────────────────────────────────────
[Code]

var
  LordOfMiceInfoShown: Boolean;

{ Check for WSL2 — needed for Tmux and Codex features.
  Not a hard requirement: the app works fine without it for basic mapping. }
function IsWSL2Present(): Boolean;
var
  ResultCode: Integer;
begin
  // Use {sysnative} to bypass WOW64 filesystem redirection: the Inno Setup
  // installer is 32-bit, so {sys} resolves to SysWOW64 which does NOT contain
  // wsl.exe. {sysnative} always points to the real System32.
  // Run "wsl -e true" instead of "wsl --status": --status has unreliable exit
  // codes on some machines; -e true returns 0 iff WSL is functional.
  Result := Exec(ExpandConstant('{sysnative}\wsl.exe'), '-e true', '',
                 SW_HIDE, ewWaitUntilTerminated, ResultCode)
            and (ResultCode = 0);
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  ResultCode: Integer;
begin
  if CurUninstallStep = usUninstall then
  begin
    // Kill running DS4CC before removing files — it runs as a hidden background process
    Exec('taskkill', '/F /IM ds4cc.exe', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  ResultCode: Integer;
begin
  // Kill running DS4CC before overwriting the binary
  Exec('taskkill', '/F /IM ds4cc.exe', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Result := '';
end;

{ Explain LordOfMice once when the user reaches the Select Tasks page.
  Task is checked by default; they uncheck if they do not want it. }
procedure CurPageChanged(CurPageID: Integer);
begin
  if (CurPageID = wpSelectTasks) and (not LordOfMiceInfoShown) then
  begin
    LordOfMiceInfoShown := True;
    MsgBox(
      'Optional: LordOfMice hidusbf (USB HID buffering / overclock)' + #13#10 + #13#10 +
      'Highly recommended for DualShock 4 and DualSense on Windows.' + #13#10 +
      'It is a net positive for gaming and for low-latency controller input' + #13#10 +
      'in general — including omegaG / DS4CC shortcut mapping.' + #13#10 + #13#10 +
      'Ballpark (USB HID path / community measurements):' + #13#10 +
      '  · DualShock 4: ~200 ms class input lag without overclock tooling' + #13#10 +
      '  · DualSense @ up to 8000 Hz polling: on the order of ~0.25 ms' + #13#10 +
      '    with buffering / overclock configured correctly' + #13#10 + #13#10 +
      'We do not bundle the driver. If you leave the next checkbox ON, Finish' + #13#10 +
      'will open the official LordOfMice/hidusbf project so you can install it.' + #13#10 +
      'Uncheck that task if you do not want this.' + #13#10 + #13#10 +
      'Third-party kernel filter: review the project and install only if you trust it.',
      mbInformation, MB_OK);
  end;
end;

procedure InitializeWizard();
begin
  LordOfMiceInfoShown := False;
  if not IsWSL2Present() then
  begin
    MsgBox(
      'WSL2 was not detected on this machine.' + #13#10 + #13#10 +
      'DS4CC will work normally for controller mapping,' + #13#10 +
      'but the Tmux and Codex (AI agent) integrations require WSL2.' + #13#10 + #13#10 +
      'You can install WSL2 at any time from the Microsoft Store, ' +
      'or by running:' + #13#10 +
      '    wsl --install' + #13#10 + #13#10 +
      'The installation will continue regardless.',
      mbInformation, MB_OK);
  end;
end;
