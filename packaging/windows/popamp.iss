; Popamp's Windows installer, built by packaging/build-windows.sh with
; Inno Setup 6. It gets: Version, Title ("Popamp" or "Popamp Plus"),
; Source (the folder with popamp.exe and its papers), OutputName, OutputDir.
; Both builds share one AppId, so installing one replaces the other.
; Settings stay in %APPDATA%\popamp when it is uninstalled.

[Setup]
AppId={{6D1F4C2A-8E3B-4F7A-9C55-2B7E0A4D9F13}
AppName={#Title}
AppVersion={#Version}
AppVerName={#Title} {#Version}
AppPublisher=Popamp
AppPublisherURL=https://github.com/popamp/popamp
AppSupportURL=https://github.com/popamp/popamp/issues
AppUpdatesURL=https://github.com/popamp/popamp/releases
DefaultDirName={autopf}\Popamp
DisableProgramGroupPage=yes
; For this user without asking for administrator rights, or for all users.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
ChangesAssociations=yes
SetupIconFile=..\..\assets\logo\popamp.ico
UninstallDisplayIcon={app}\popamp.exe
UninstallDisplayName={#Title}
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes
OutputDir={#OutputDir}
OutputBaseFilename={#OutputName}

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "ru"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "uk"; MessagesFile: "compiler:Languages\Ukrainian.isl"
Name: "de"; MessagesFile: "compiler:Languages\German.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#Source}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs

[Icons]
Name: "{autoprograms}\{#Title}"; Filename: "{app}\popamp.exe"
Name: "{autodesktop}\{#Title}"; Filename: "{app}\popamp.exe"; Tasks: desktopicon

[Registry]
; Audio files and playlists get Popamp in "Open with"; Windows lets only
; the user make it the default player. Skins open in Popamp, which
; installs them.
Root: HKA; Subkey: "Software\Classes\Popamp.Audio"; ValueType: string; ValueName: ""; ValueData: "Audio file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Popamp.Audio\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\popamp.exe,0"
Root: HKA; Subkey: "Software\Classes\Popamp.Audio\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\popamp.exe"" ""%1"""
Root: HKA; Subkey: "Software\Classes\Popamp.Skin"; ValueType: string; ValueName: ""; ValueData: "Popamp skin"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Popamp.Skin\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\popamp.exe,0"
Root: HKA; Subkey: "Software\Classes\Popamp.Skin\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\popamp.exe"" ""%1"""
Root: HKA; Subkey: "Software\Classes\Applications\popamp.exe"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "{#Title}"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Applications\popamp.exe\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\popamp.exe"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.mp3\OpenWithProgids"; ValueType: string; ValueName: "Popamp.Audio"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.mp2\OpenWithProgids"; ValueType: string; ValueName: "Popamp.Audio"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.mp1\OpenWithProgids"; ValueType: string; ValueName: "Popamp.Audio"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.aac\OpenWithProgids"; ValueType: string; ValueName: "Popamp.Audio"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.m4a\OpenWithProgids"; ValueType: string; ValueName: "Popamp.Audio"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.m4b\OpenWithProgids"; ValueType: string; ValueName: "Popamp.Audio"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.alac\OpenWithProgids"; ValueType: string; ValueName: "Popamp.Audio"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.flac\OpenWithProgids"; ValueType: string; ValueName: "Popamp.Audio"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.ogg\OpenWithProgids"; ValueType: string; ValueName: "Popamp.Audio"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.oga\OpenWithProgids"; ValueType: string; ValueName: "Popamp.Audio"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.wav\OpenWithProgids"; ValueType: string; ValueName: "Popamp.Audio"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.wave\OpenWithProgids"; ValueType: string; ValueName: "Popamp.Audio"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.aif\OpenWithProgids"; ValueType: string; ValueName: "Popamp.Audio"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.aiff\OpenWithProgids"; ValueType: string; ValueName: "Popamp.Audio"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.aifc\OpenWithProgids"; ValueType: string; ValueName: "Popamp.Audio"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.caf\OpenWithProgids"; ValueType: string; ValueName: "Popamp.Audio"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.mka\OpenWithProgids"; ValueType: string; ValueName: "Popamp.Audio"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.m3u\OpenWithProgids"; ValueType: string; ValueName: "Popamp.Audio"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.m3u8\OpenWithProgids"; ValueType: string; ValueName: "Popamp.Audio"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.pls\OpenWithProgids"; ValueType: string; ValueName: "Popamp.Audio"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.xspf\OpenWithProgids"; ValueType: string; ValueName: "Popamp.Audio"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.wsz"; ValueType: string; ValueName: ""; ValueData: "Popamp.Skin"; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.psz"; ValueType: string; ValueName: ""; ValueData: "Popamp.Skin"; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\popamp.exe\SupportedTypes"; ValueType: string; ValueName: ".mp3"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\popamp.exe\SupportedTypes"; ValueType: string; ValueName: ".mp2"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\popamp.exe\SupportedTypes"; ValueType: string; ValueName: ".mp1"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\popamp.exe\SupportedTypes"; ValueType: string; ValueName: ".aac"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\popamp.exe\SupportedTypes"; ValueType: string; ValueName: ".m4a"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\popamp.exe\SupportedTypes"; ValueType: string; ValueName: ".m4b"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\popamp.exe\SupportedTypes"; ValueType: string; ValueName: ".alac"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\popamp.exe\SupportedTypes"; ValueType: string; ValueName: ".flac"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\popamp.exe\SupportedTypes"; ValueType: string; ValueName: ".ogg"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\popamp.exe\SupportedTypes"; ValueType: string; ValueName: ".oga"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\popamp.exe\SupportedTypes"; ValueType: string; ValueName: ".wav"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\popamp.exe\SupportedTypes"; ValueType: string; ValueName: ".wave"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\popamp.exe\SupportedTypes"; ValueType: string; ValueName: ".aif"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\popamp.exe\SupportedTypes"; ValueType: string; ValueName: ".aiff"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\popamp.exe\SupportedTypes"; ValueType: string; ValueName: ".aifc"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\popamp.exe\SupportedTypes"; ValueType: string; ValueName: ".caf"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\popamp.exe\SupportedTypes"; ValueType: string; ValueName: ".mka"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\popamp.exe\SupportedTypes"; ValueType: string; ValueName: ".m3u"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\popamp.exe\SupportedTypes"; ValueType: string; ValueName: ".m3u8"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\popamp.exe\SupportedTypes"; ValueType: string; ValueName: ".pls"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\popamp.exe\SupportedTypes"; ValueType: string; ValueName: ".xspf"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\popamp.exe\SupportedTypes"; ValueType: string; ValueName: ".wsz"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\popamp.exe\SupportedTypes"; ValueType: string; ValueName: ".psz"; ValueData: ""

; Popamp in Settings > Default apps, where the user can make it the
; player for each type.
Root: HKA; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "Popamp"; ValueData: "Software\Popamp\Capabilities"; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Popamp"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Popamp\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "{#Title}"
Root: HKA; Subkey: "Software\Popamp\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "Skinnable audio player"
Root: HKA; Subkey: "Software\Popamp\Capabilities"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: "{app}\popamp.exe,0"
Root: HKA; Subkey: "Software\Popamp\Capabilities\FileAssociations"; ValueType: string; ValueName: ".mp3"; ValueData: "Popamp.Audio"
Root: HKA; Subkey: "Software\Popamp\Capabilities\FileAssociations"; ValueType: string; ValueName: ".mp2"; ValueData: "Popamp.Audio"
Root: HKA; Subkey: "Software\Popamp\Capabilities\FileAssociations"; ValueType: string; ValueName: ".mp1"; ValueData: "Popamp.Audio"
Root: HKA; Subkey: "Software\Popamp\Capabilities\FileAssociations"; ValueType: string; ValueName: ".aac"; ValueData: "Popamp.Audio"
Root: HKA; Subkey: "Software\Popamp\Capabilities\FileAssociations"; ValueType: string; ValueName: ".m4a"; ValueData: "Popamp.Audio"
Root: HKA; Subkey: "Software\Popamp\Capabilities\FileAssociations"; ValueType: string; ValueName: ".m4b"; ValueData: "Popamp.Audio"
Root: HKA; Subkey: "Software\Popamp\Capabilities\FileAssociations"; ValueType: string; ValueName: ".alac"; ValueData: "Popamp.Audio"
Root: HKA; Subkey: "Software\Popamp\Capabilities\FileAssociations"; ValueType: string; ValueName: ".flac"; ValueData: "Popamp.Audio"
Root: HKA; Subkey: "Software\Popamp\Capabilities\FileAssociations"; ValueType: string; ValueName: ".ogg"; ValueData: "Popamp.Audio"
Root: HKA; Subkey: "Software\Popamp\Capabilities\FileAssociations"; ValueType: string; ValueName: ".oga"; ValueData: "Popamp.Audio"
Root: HKA; Subkey: "Software\Popamp\Capabilities\FileAssociations"; ValueType: string; ValueName: ".wav"; ValueData: "Popamp.Audio"
Root: HKA; Subkey: "Software\Popamp\Capabilities\FileAssociations"; ValueType: string; ValueName: ".wave"; ValueData: "Popamp.Audio"
Root: HKA; Subkey: "Software\Popamp\Capabilities\FileAssociations"; ValueType: string; ValueName: ".aif"; ValueData: "Popamp.Audio"
Root: HKA; Subkey: "Software\Popamp\Capabilities\FileAssociations"; ValueType: string; ValueName: ".aiff"; ValueData: "Popamp.Audio"
Root: HKA; Subkey: "Software\Popamp\Capabilities\FileAssociations"; ValueType: string; ValueName: ".aifc"; ValueData: "Popamp.Audio"
Root: HKA; Subkey: "Software\Popamp\Capabilities\FileAssociations"; ValueType: string; ValueName: ".caf"; ValueData: "Popamp.Audio"
Root: HKA; Subkey: "Software\Popamp\Capabilities\FileAssociations"; ValueType: string; ValueName: ".mka"; ValueData: "Popamp.Audio"
Root: HKA; Subkey: "Software\Popamp\Capabilities\FileAssociations"; ValueType: string; ValueName: ".m3u"; ValueData: "Popamp.Audio"
Root: HKA; Subkey: "Software\Popamp\Capabilities\FileAssociations"; ValueType: string; ValueName: ".m3u8"; ValueData: "Popamp.Audio"
Root: HKA; Subkey: "Software\Popamp\Capabilities\FileAssociations"; ValueType: string; ValueName: ".pls"; ValueData: "Popamp.Audio"
Root: HKA; Subkey: "Software\Popamp\Capabilities\FileAssociations"; ValueType: string; ValueName: ".xspf"; ValueData: "Popamp.Audio"
Root: HKA; Subkey: "Software\Popamp\Capabilities\FileAssociations"; ValueType: string; ValueName: ".wsz"; ValueData: "Popamp.Skin"
Root: HKA; Subkey: "Software\Popamp\Capabilities\FileAssociations"; ValueType: string; ValueName: ".psz"; ValueData: "Popamp.Skin"

[Run]
Filename: "{app}\popamp.exe"; Description: "{cm:LaunchProgram,{#Title}}"; Flags: nowait postinstall skipifsilent
