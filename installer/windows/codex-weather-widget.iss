#define AppName "Codex Weather Widget"
#ifndef AppVersion
#define AppVersion "dev"
#endif
#ifndef SourceDir
#define SourceDir "..\..\target\release"
#endif
#ifndef OutputDir
#define OutputDir "..\..\dist"
#endif

[Setup]
AppId={{2F6B1556-C441-4AA6-87B5-109A59E6027A}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=adoin
AppPublisherURL=https://github.com/adoin/Codex-weather-widget
AppSupportURL=https://github.com/adoin/Codex-weather-widget/issues
AppUpdatesURL=https://github.com/adoin/Codex-weather-widget/releases
DefaultDirName={localappdata}\Programs\CodexWeatherWidget
DisableDirPage=no
DisableProgramGroupPage=no
DefaultGroupName=Codex Weather Widget
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#OutputDir}
OutputBaseFilename=CodexWeatherWidgetSetup-{#AppVersion}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
SetupIconFile=..\..\assets\icons\codex-weather-widget.ico
UninstallDisplayIcon={app}\codex-weather-widget.exe
LicenseFile=..\..\LICENSE
; The widget's normal window close hides it to the tray. Ask its control API
; to exit first (PrepareToInstall below), then let Restart Manager terminate
; a hung instance. Only register executables, never documents or skill files.
CloseApplications=force
CloseApplicationsFilter=*.exe
RestartApplications=no
SetupLogging=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Files]
Source: "{#SourceDir}\codex-calendar.exe"; DestDir: "{app}"; DestName: "codex-weather-widget.exe"; Flags: ignoreversion
Source: "..\..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\THIRD_PARTY_NOTICES.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\config.example.json"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\codex-plugin\quota-calendar\skills\quota\SKILL.md"; DestDir: "{%USERPROFILE}\.codex\skills\quota"; Flags: ignoreversion
Source: "..\..\codex-plugin\quota-calendar\skills\quota\scripts\*"; DestDir: "{%USERPROFILE}\.codex\skills\quota\scripts"; Flags: ignoreversion recursesubdirs createallsubdirs

[InstallDelete]
; Remove the shortcut created by pre-1.0.1 development installers so the
; in-app startup switch remains the single source of truth.
Type: files; Name: "{userstartup}\Codex Calendar.lnk"

[Icons]
Name: "{group}\Codex Weather Widget"; Filename: "{app}\codex-weather-widget.exe"; WorkingDir: "{app}"
Name: "{group}\Uninstall Codex Weather Widget"; Filename: "{uninstallexe}"

[Run]
Filename: "{app}\codex-weather-widget.exe"; Description: "{cm:LaunchProgram,{#StringChange(AppName, '&', '&&')}}"; WorkingDir: "{app}"; Flags: nowait postinstall skipifsilent

[Code]
function WidgetControlPort: Integer;
var
  ConfigPath, Tail, Digits: String;
  Config: AnsiString;
  KeyPos, ColonPos, I: Integer;
begin
  Result := 8787;
  ConfigPath := GetEnv('CODEX_CALENDAR_CONFIG');
  if ConfigPath = '' then
    ConfigPath := ExpandConstant('{userappdata}\CodexCalendar\config.json');
  if not LoadStringFromFile(ConfigPath, Config) then
    Exit;
  KeyPos := Pos('"port"', String(Config));
  if KeyPos = 0 then
    Exit;
  Tail := Copy(String(Config), KeyPos + 6, Length(Config));
  ColonPos := Pos(':', Tail);
  if ColonPos = 0 then
    Exit;
  Tail := Trim(Copy(Tail, ColonPos + 1, Length(Tail)));
  Digits := '';
  I := 1;
  while I <= Length(Tail) do
  begin
    if (Tail[I] < '0') or (Tail[I] > '9') then
      Break;
    Digits := Digits + Tail[I];
    I := I + 1;
  end;
  Result := StrToIntDef(Digits, 8787);
  if (Result < 1) or (Result > 65535) then
    Result := 8787;
end;

procedure RequestWidgetExit;
var
  Request: Variant;
  BaseURL, Health: String;
begin
  try
    BaseURL := 'http://127.0.0.1:' + IntToStr(WidgetControlPort);
    Request := CreateOleObject('WinHttp.WinHttpRequest.5.1');
    Request.SetProxy(1);
    Request.SetTimeouts(500, 500, 500, 500);
    Request.Open('GET', BaseURL + '/health', False);
    Request.Send('');
    Health := Request.ResponseText;
    if (Request.Status <> 200) or
       (Pos('"app":"codex-calendar"', Health) = 0) then
      Exit;
    Request.Open('POST', BaseURL + '/panel/quit', False);
    Request.Send('');
    Log('Requested graceful widget exit, HTTP status ' + IntToStr(Request.Status));
    Sleep(1500);
  except
    Log('Widget control API unavailable; using executable-only Restart Manager fallback.');
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  RequestWidgetExit;
  Result := '';
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  ResultCode: Integer;
begin
  if CurUninstallStep = usUninstall then
  begin
    RequestWidgetExit;
    Exec(ExpandConstant('{sys}\schtasks.exe'),
      '/Delete /TN "Codex Weather Widget" /F', '', SW_HIDE,
      ewWaitUntilTerminated, ResultCode);
    DeleteFile(ExpandConstant('{userstartup}\Codex Calendar.lnk'));
  end;
end;
