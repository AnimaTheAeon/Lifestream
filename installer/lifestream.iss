; Lifestream installer (Inno Setup 6). Built by GitHub Actions:
;   iscc /DMyAppVersion=x.y.z installer\lifestream.iss

#ifndef MyAppVersion
  #define MyAppVersion "0.0.0"
#endif
#define MyAppName "Lifestream"
#define MyAppExe "lifestream.exe"
#define MyAppPublisher "AnimaTheAeon"
#define MyAppURL "https://github.com/AnimaTheAeon/Lifestream"

[Setup]
AppId={{6F3C2A9E-4B1D-4E7A-9C55-1A7E0F2B8D41}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}/issues
AppUpdatesURL={#MyAppURL}/releases
; Per-user install: no admin prompt, lands in %LOCALAPPDATA%\Programs\Lifestream.
PrivilegesRequired=lowest
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
UninstallDisplayIcon={app}\{#MyAppExe}
UninstallDisplayName={#MyAppName}
SetupIconFile=..\resources\lifestream.ico
; Tell Explorer the icons changed (shortcuts, file types, magnet links).
ChangesAssociations=yes
WizardStyle=modern
WizardSmallImageFile=wizard-small.bmp
WizardImageFile=wizard-large.bmp
OutputDir=..\target\installer
OutputBaseFilename=Lifestream-Setup-{#MyAppVersion}
Compression=lzma2/max
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Close a running copy before replacing files.
CloseApplications=force
RestartApplications=no
AppMutex=LifestreamSingleInstance

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Shortcuts:"
Name: "associate"; Description: "Open &magnet links and .torrent files with Lifestream"; GroupDescription: "Integration:"
Name: "autostart"; Description: "Start Lifestream with &Windows (in the tray)"; GroupDescription: "Integration:"; Flags: unchecked

[Files]
Source: "..\target\release\{#MyAppExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExe}"; Tasks: desktopicon

[Registry]
; Magnet links
Root: HKCU; Subkey: "Software\Classes\magnet"; ValueType: string; ValueName: ""; ValueData: "URL:Magnet link"; Flags: uninsdeletekey; Tasks: associate
Root: HKCU; Subkey: "Software\Classes\magnet"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""; Tasks: associate
Root: HKCU; Subkey: "Software\Classes\magnet\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExe}"",0"; Tasks: associate
Root: HKCU; Subkey: "Software\Classes\magnet\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExe}"" ""%1"""; Tasks: associate
; .torrent files
Root: HKCU; Subkey: "Software\Classes\Lifestream.Torrent"; ValueType: string; ValueName: ""; ValueData: "Torrent file"; Flags: uninsdeletekey; Tasks: associate
Root: HKCU; Subkey: "Software\Classes\Lifestream.Torrent\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExe}"",0"; Tasks: associate
Root: HKCU; Subkey: "Software\Classes\Lifestream.Torrent\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExe}"" ""%1"""; Tasks: associate
Root: HKCU; Subkey: "Software\Classes\.torrent"; ValueType: string; ValueName: ""; ValueData: "Lifestream.Torrent"; Flags: uninsdeletevalue; Tasks: associate
Root: HKCU; Subkey: "Software\Classes\.torrent"; ValueType: string; ValueName: "Content Type"; ValueData: "application/x-bittorrent"; Tasks: associate
; Start with Windows (the app keeps this in sync with its own setting)
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "Lifestream"; ValueData: """{app}\{#MyAppExe}"" --tray"; Tasks: autostart
; Always remove the autostart entry on uninstall, however it was created.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "Lifestream"; Flags: uninsdeletevalue

[Run]
; Refresh Windows' icon cache so shortcuts and pinned taskbar icons show the new icon.
Filename: "{sys}\ie4uinit.exe"; Parameters: "-show"; Flags: runhidden waituntilterminated skipifdoesntexist
Filename: "{app}\{#MyAppExe}"; Description: "Launch Lifestream"; Flags: nowait postinstall skipifsilent
; In-app updates run the installer silently; reopen Lifestream afterwards.
Filename: "{app}\{#MyAppExe}"; Parameters: "--after-update"; Flags: nowait; Check: WizardSilent

[Code]
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  DataDir: String;
begin
  if CurUninstallStep = usPostUninstall then
  begin
    DataDir := ExpandConstant('{userappdata}\Lifestream');
    if DirExists(DataDir) and not UninstallSilent then
      if MsgBox('Also delete your Lifestream settings, RSS rules and torrent list?' + #13#10 + #13#10 +
                'Your downloaded files are not touched either way.',
                mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
        DelTree(DataDir, True, True, True);
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  // The app reads this to keep its "Start with Windows" switch in step.
  if (CurStep = ssPostInstall) and WizardIsTaskSelected('autostart') then
  begin
    ForceDirectories(ExpandConstant('{userappdata}\Lifestream'));
    SaveStringToFile(ExpandConstant('{userappdata}\Lifestream\installer-autostart'), '1', False);
  end;
end;
