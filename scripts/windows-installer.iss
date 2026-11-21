; Inno Setup 6 installer script for Manga TUI (Windows x64)
; Builds a per-user (or optional all-users) installer with shortcuts, uninstaller,
; optional PATH registration, and guaranteed user-data preservation on upgrade/uninstall.

#ifndef MyAppVersion
  #define MyAppVersion "0.3.0"
#endif

#ifndef MyAppTarget
  #define MyAppTarget "x86_64-pc-windows-msvc"
#endif

#ifndef MyAppExePath
  #define MyAppExePath "..\target\release\manga-tui.exe"
#endif

#ifndef MyOutputDir
  #define MyOutputDir "..\dist"
#endif

#ifndef MyOutputBaseFilename
  #define MyOutputBaseFilename "manga-tui-" + MyAppVersion + "-" + MyAppTarget + "-setup"
#endif

#define MyAppName "Manga TUI"
#define MyAppExeName "manga-tui.exe"
#define MyAppPublisher "tomahawk1103 & drodri22"
#define MyAppURL "https://github.com/tomahawk1103/manga-tui"

[Setup]
AppId={{B7F4E2A9-8C31-4D62-9A5E-1D8F6B3C2E40}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}/issues
AppUpdatesURL={#MyAppURL}/releases
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#MyOutputDir}
OutputBaseFilename={#MyOutputBaseFilename}
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
UninstallDisplayName={#MyAppName}
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
ChangesEnvironment=yes
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
spanish.AddToPathTask=Añadir Manga TUI al PATH (permite ejecutar "manga-tui" desde cualquier terminal)
english.AddToPathTask=Add Manga TUI to PATH (run "manga-tui" from any terminal)
spanish.DesktopIconTask=Crear un acceso directo en el Escritorio
english.DesktopIconTask=Create a Desktop shortcut
spanish.LaunchApp=Ejecutar {#MyAppName}
english.LaunchApp=Launch {#MyAppName}
spanish.AdditionalTasksGroup=Opciones adicionales:
english.AdditionalTasksGroup=Additional options:

[Tasks]
Name: "desktopicon"; Description: "{cm:DesktopIconTask}"; GroupDescription: "{cm:AdditionalTasksGroup}"
Name: "addtopath"; Description: "{cm:AddToPathTask}"; GroupDescription: "{cm:AdditionalTasksGroup}"

[Files]
; NOTE: Intentionally do NOT ship portable.txt or data/ in the installer so that
; installed manga-tui stores user data in %APPDATA%\manga-tui (outside {app}),
; guaranteeing config.toml, SQLite history, and downloads survive upgrades & uninstalls.
Source: "{#MyAppExePath}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#MyAppName}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\{#MyAppExeName}"
Name: "{autoprograms}\{#MyAppName}\{cm:UninstallProgram,{#MyAppName}}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchApp}"; Flags: nowait postinstall skipifsilent

[Code]
function EnvRootKey: Integer;
begin
  if IsAdminInstallMode then
    Result := HKEY_LOCAL_MACHINE
  else
    Result := HKEY_CURRENT_USER;
end;

function EnvSubKeyName: String;
begin
  if IsAdminInstallMode then
    Result := 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment'
  else
    Result := 'Environment';
end;

function NormalizePathSegment(const Value: String): String;
begin
  Result := Trim(Value);
  while (Length(Result) > 1) and (Result[Length(Result)] = '\') do
    Delete(Result, Length(Result), 1);
end;

function PathContainsDir(const PathValue, DirPath: String): Boolean;
var
  Remaining, Segment, Target: String;
  SepPos: Integer;
begin
  Result := False;
  Target := UpperCase(NormalizePathSegment(DirPath));
  if Target = '' then
    Exit;

  Remaining := PathValue;
  while Remaining <> '' do
  begin
    SepPos := Pos(';', Remaining);
    if SepPos > 0 then
    begin
      Segment := Copy(Remaining, 1, SepPos - 1);
      Delete(Remaining, 1, SepPos);
    end
    else
    begin
      Segment := Remaining;
      Remaining := '';
    end;

    if UpperCase(NormalizePathSegment(Segment)) = Target then
    begin
      Result := True;
      Exit;
    end;
  end;
end;

procedure AddDirToPath(const DirPath: String);
var
  CurrentPath, NormalizedDir, NewPath: String;
begin
  NormalizedDir := NormalizePathSegment(DirPath);
  if NormalizedDir = '' then
    Exit;

  if not RegQueryStringValue(EnvRootKey, EnvSubKeyName, 'Path', CurrentPath) then
    CurrentPath := '';

  if PathContainsDir(CurrentPath, NormalizedDir) then
    Exit;

  if CurrentPath = '' then
    NewPath := NormalizedDir
  else if CurrentPath[Length(CurrentPath)] = ';' then
    NewPath := CurrentPath + NormalizedDir
  else
    NewPath := CurrentPath + ';' + NormalizedDir;

  RegWriteExpandStringValue(EnvRootKey, EnvSubKeyName, 'Path', NewPath);
end;

procedure RemoveDirFromPath(const DirPath: String);
var
  CurrentPath, Remaining, Segment, Target, Rebuilt: String;
  SepPos: Integer;
  Changed: Boolean;
begin
  Target := UpperCase(NormalizePathSegment(DirPath));
  if Target = '' then
    Exit;

  if not RegQueryStringValue(EnvRootKey, EnvSubKeyName, 'Path', CurrentPath) then
    Exit;

  Rebuilt := '';
  Remaining := CurrentPath;
  Changed := False;

  while Remaining <> '' do
  begin
    SepPos := Pos(';', Remaining);
    if SepPos > 0 then
    begin
      Segment := Copy(Remaining, 1, SepPos - 1);
      Delete(Remaining, 1, SepPos);
    end
    else
    begin
      Segment := Remaining;
      Remaining := '';
    end;

    if UpperCase(NormalizePathSegment(Segment)) = Target then
    begin
      Changed := True;
    end
    else if Trim(Segment) <> '' then
    begin
      if Rebuilt <> '' then
        Rebuilt := Rebuilt + ';';
      Rebuilt := Rebuilt + Segment;
    end;
  end;

  if Changed then
    RegWriteExpandStringValue(EnvRootKey, EnvSubKeyName, 'Path', Rebuilt);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if (CurStep = ssPostInstall) and WizardIsTaskSelected('addtopath') then
  begin
    AddDirToPath(ExpandConstant('{app}'));
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
  begin
    RemoveDirFromPath(ExpandConstant('{app}'));
  end;
end;
