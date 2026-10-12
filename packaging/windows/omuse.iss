; A per-user installer for an already verified, complete Windows archive.
; Keep this AppId stable across versions. Never delete user data on uninstall.
#ifndef AppVersion
  #error AppVersion is required
#endif
#ifndef PayloadDir
  #error PayloadDir is required
#endif
#ifndef OutputDir
  #error OutputDir is required
#endif

[Setup]
AppId={{A6FD8840-423A-4C49-BDFC-EE180DCE143B}
AppName=Omuse
AppVersion={#AppVersion}
AppPublisher=Sugata Software
AppPublisherURL=https://github.com/Sugata-Software/Omuse
AppSupportURL=https://github.com/Sugata-Software/Omuse/issues
AppUpdatesURL=https://github.com/Sugata-Software/Omuse/releases
DefaultDirName={localappdata}\Programs\Omuse
DefaultGroupName=Omuse
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64os
ArchitecturesInstallIn64BitMode=x64os
MinVersion=10.0
WizardStyle=modern
SetupIconFile=..\..\rust\assets\windows\omuse.ico
UninstallDisplayIcon={app}\omuse.exe
LicenseFile={#PayloadDir}\licenses\Omuse-MIT.txt
InfoBeforeFile=installer-info.txt
OutputDir={#OutputDir}
OutputBaseFilename=omuse-{#AppVersion}-windows-x86_64-setup
Compression=lzma2
SolidCompression=yes
CloseApplications=no
RestartApplications=no
UsePreviousAppDir=yes
UsePreviousTasks=yes
Uninstallable=yes
VersionInfoVersion={#AppVersion}.0
VersionInfoDescription=Omuse Setup

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked

[Files]
Source: "{#PayloadDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\Omuse"; Filename: "{app}\omuse.exe"; WorkingDir: "{app}"
Name: "{autodesktop}\Omuse"; Filename: "{app}\omuse.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\omuse.exe"; Description: "Launch Omuse"; Flags: nowait postinstall skipifsilent
Filename: "https://github.com/Sugata-Software/Omuse/blob/main/docs/windows-installer.md"; Description: "Read the Windows setup guide (optional tools and AI connections)"; Flags: shellexec postinstall skipifsilent unchecked

[Code]
function NextVersionPart(var Value: String): Integer;
var P: Integer; Part: String;
begin
  P := Pos('.', Value);
  if P = 0 then begin Part := Value; Value := ''; end
  else begin Part := Copy(Value, 1, P - 1); Delete(Value, 1, P); end;
  Result := StrToIntDef(Part, 0);
end;

function IsNewerVersion(Installed, Candidate: String): Boolean;
var I, A, B: Integer;
begin
  Result := False;
  for I := 1 to 3 do begin
    A := NextVersionPart(Installed); B := NextVersionPart(Candidate);
    if A <> B then begin Result := A > B; Exit; end;
  end;
end;

function InitializeSetup(): Boolean;
var Installed: String;
begin
  Result := True;
  if RegQueryStringValue(HKCU64,
    'Software\Microsoft\Windows\CurrentVersion\Uninstall\{A6FD8840-423A-4C49-BDFC-EE180DCE143B}_is1',
    'DisplayVersion', Installed) and IsNewerVersion(Installed, '{#AppVersion}') then begin
    SuppressibleMsgBox('A newer version of Omuse is installed. To use an older version, uninstall Omuse first. Your projects and settings will be kept. Older versions may not open newer project formats.', mbError, MB_OK, IDOK);
    Result := False;
  end;
end;
