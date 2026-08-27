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
CloseApplications=yes
RestartApplications=no

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
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  ResultCode: Integer;
begin
  if CurUninstallStep = usUninstall then
  begin
    Exec(ExpandConstant('{sys}\schtasks.exe'),
      '/Delete /TN "Codex Weather Widget" /F', '', SW_HIDE,
      ewWaitUntilTerminated, ResultCode);
    DeleteFile(ExpandConstant('{userstartup}\Codex Calendar.lnk'));
  end;
end;
