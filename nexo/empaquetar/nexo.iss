; Proyecto: NEXO
; Archivo: nexo.iss
; Autor: Chatta Betanzos, Jimy Alexander (jimy-betanzos)
;
; Instalador clásico de Windows (Inno Setup 6): Bienvenida → Información →
; Carpeta → Accesos directos → Instalar → Finalizar. Se instala en el perfil del
; usuario (no pide contraseña de administrador) y aparece en
; Configuración → Aplicaciones instaladas, desde donde se puede desinstalar.
;
; La carpeta y el acceso del menú Inicio son los mismos que usa NEXO cuando se
; instala solo (src/install.rs), así nunca quedan dos copias.
;
; Compilar (después de «cargo build --release», dentro de la carpeta nexo):
;   target\release\nexo.exe --exportar-ico empaquetar\nexo.ico
;   "C:\Program Files (x86)\Inno Setup 6\ISCC.exe" /DMyVersion=1.5.0 empaquetar\nexo.iss
; Resultado: empaquetar\salida\NEXO-Instalador-Windows.exe

#ifndef MyVersion
  #define MyVersion "0.0.0"
#endif

[Setup]
AppId={{7E3C2A51-9B4D-4E8F-A1C6-3D5B9F0E2A47}
AppName=NEXO
AppVersion={#MyVersion}
AppVerName=NEXO {#MyVersion}
AppPublisher=Chatta Betanzos, Jimy Alexander
AppPublisherURL=https://github.com/JIMY-BETANZOS-1/nexo
AppSupportURL=https://github.com/JIMY-BETANZOS-1/nexo
AppUpdatesURL=https://github.com/JIMY-BETANZOS-1/nexo/releases/latest
AppCopyright=© 2026 Chatta Betanzos, Jimy Alexander (jimy-betanzos)
VersionInfoVersion={#MyVersion}
VersionInfoCompany=jimy-betanzos
VersionInfoDescription=Instalador de NEXO
DefaultDirName={localappdata}\Programs\NEXO
DisableDirPage=no
DisableProgramGroupPage=yes
DisableWelcomePage=no
PrivilegesRequired=lowest
OutputDir=salida
OutputBaseFilename=NEXO-Instalador-Windows
SetupIconFile=nexo.ico
UninstallDisplayIcon={app}\nexo.exe
UninstallDisplayName=NEXO
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
InfoBeforeFile=antes.txt
CloseApplications=yes
; Si la ventana de NEXO está abierta, el asistente pide cerrarla antes de seguir.
AppMutex=NEXO_INTERFAZ

[Languages]
Name: "es"; MessagesFile: "compiler:Languages\Spanish.isl"

[Tasks]
Name: "escritorio"; Description: "Crear un acceso directo en el escritorio"; GroupDescription: "Accesos directos:"

[Files]
Source: "..\target\release\nexo.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "nexo.ico"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{userprograms}\NEXO"; Filename: "{app}\nexo.exe"; IconFilename: "{app}\nexo.ico"; Comment: "NEXO - limpieza y optimización local"
Name: "{userdesktop}\NEXO"; Filename: "{app}\nexo.exe"; IconFilename: "{app}\nexo.ico"; Comment: "NEXO - limpieza y optimización local"; Tasks: escritorio

[Run]
; «Abrir NEXO ahora» en la última pantalla. shellexec permite que NEXO pida su permiso de administrador.
Filename: "{app}\nexo.exe"; Description: "Abrir NEXO ahora"; Flags: nowait postinstall skipifsilent shellexec

[UninstallRun]
; Antes de borrar los archivos, NEXO quita su tarea programada, el vigía y su arranque con Windows.
; El historial y la configuración (%APPDATA%\NEXO) se conservan.
Filename: "{app}\nexo.exe"; Parameters: "--desinstalar ""{tmp}\nexo-desinstalar.txt"""; Flags: runhidden waituntilterminated; RunOnceId: "QuitarNEXO"

[Code]
// jimy-betanzos: el vigía (ícono junto al reloj) usa nexo.exe; si sigue abierto no se puede reemplazar.
function VigiaAbierto(): Boolean;
begin
  Result := FindWindowByClassName('NEXO_VIGIA') <> 0;
end;

function PedirCerrarVigia(): Boolean;
begin
  Result := True;
  while VigiaAbierto() do
  begin
    if MsgBox('El vigía de NEXO está abierto (ícono de NEXO junto al reloj).' + #13#10 + #13#10 +
              'Ciérralo con clic derecho sobre el ícono > Cerrar el vigía, y luego presiona Aceptar.', mbInformation, MB_OKCANCEL) = IDCANCEL then
    begin
      Result := False;
      exit;
    end;
  end;
end;

function InitializeSetup(): Boolean;
begin
  Result := PedirCerrarVigia();
end;
