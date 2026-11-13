# NEXO · diagnóstico local de solo lectura. No cambia nada del sistema.
# Devuelve un JSON con disco, batería, seguridad y programas de inicio.
#
# Cada consulta a Windows es lenta por sí sola (1-4 s), pero no dependen entre sí:
# se lanzan todas a la vez en hilos y el total tarda lo que la más lenta.
$ErrorActionPreference = 'SilentlyContinue'
$ProgressPreference = 'SilentlyContinue'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false

# Rutas donde un programa legítimo casi nunca se instala.
$bad = '(?i)\\AppData\\Local\\Temp\\|\\Windows\\Temp\\|\\Users\\Public\\|\\Downloads\\|\\AppData\\Roaming\\[^\\]+$|\\ProgramData\\[^\\]+$'

$parts = @{}

# Programas de inicio (con firma digital y estado activado/desactivado)
$parts.startup = {
    param($bad)
    $ErrorActionPreference = 'SilentlyContinue'
    function Get-Exe([string]$cmd) {
        $c = [Environment]::ExpandEnvironmentVariables($cmd).Trim()
        if ($c.StartsWith('"')) { $e = $c.IndexOf('"', 1); if ($e -gt 1) { return $c.Substring(1, $e - 1) } }
        $i = $c.ToLower().IndexOf('.exe')
        if ($i -gt 0) { return $c.Substring(0, $i + 4) }
        return ($c -split ' ')[0]
    }
    function Get-Sig([string]$exe) {
        if (-not $exe -or -not (Test-Path -LiteralPath $exe)) { return @('missing', '') }
        $s = Get-AuthenticodeSignature -LiteralPath $exe
        $pub = ''
        if ($s.SignerCertificate -and ($s.SignerCertificate.Subject -match 'CN="?([^",]+)')) { $pub = $Matches[1] }
        return @("$($s.Status)", $pub)
    }
    function Get-Approved($hive, $sub, $name) {
        $p = "$hive\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\$sub"
        $v = (Get-ItemProperty -LiteralPath $p -Name $name).$name
        if ($v -and $v.Length -gt 0) { return (($v[0] % 2) -eq 0) }
        return $true
    }
    $startup = New-Object System.Collections.ArrayList
    function Add-Entry($name, $cmd, $exe, $hive, $sub, $scope) {
        $sig = Get-Sig $exe
        [void]$startup.Add([ordered]@{
                name = "$name"; command = "$cmd"; exe = "$exe"; hive = $hive; sub = $sub; scope = $scope
                enabled = (Get-Approved $hive $sub $name); signature = $sig[0]; publisher = $sig[1]
                suspicious = ("$exe" -match $bad)
            })
    }
    foreach ($k in @(
            @('HKCU:', 'Software\Microsoft\Windows\CurrentVersion\Run', 'Run', 'usuario'),
            @('HKLM:', 'SOFTWARE\Microsoft\Windows\CurrentVersion\Run', 'Run', 'equipo'),
            @('HKLM:', 'SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run', 'Run32', 'equipo'))) {
        $key = Get-Item -LiteralPath "$($k[0])\$($k[1])"
        if (-not $key) { continue }
        foreach ($n in $key.GetValueNames()) {
            if (-not $n) { continue }
            $cmd = [string]$key.GetValue($n)
            Add-Entry $n $cmd (Get-Exe $cmd) $k[0] $k[2] $k[3]
        }
    }
    $sh = New-Object -ComObject WScript.Shell
    foreach ($f in @(
            @("$env:APPDATA\Microsoft\Windows\Start Menu\Programs\Startup", 'HKCU:', 'usuario'),
            @("$env:ProgramData\Microsoft\Windows\Start Menu\Programs\StartUp", 'HKLM:', 'equipo'))) {
        Get-ChildItem -LiteralPath $f[0] -File | Where-Object { $_.Name -ne 'desktop.ini' } | ForEach-Object {
            $t = $_.FullName
            if ($_.Extension -eq '.lnk') { $t = $sh.CreateShortcut($_.FullName).TargetPath }
            Add-Entry $_.Name $_.FullName $t $f[1] 'StartupFolder' $f[2]
        }
    }
    @{ startup = @($startup) }
}

# Discos (la salud SMART detallada puede requerir administrador)
$parts.disks = {
    $ErrorActionPreference = 'SilentlyContinue'
    $d = @(Get-PhysicalDisk | ForEach-Object {
            $rel = $_ | Get-StorageReliabilityCounter
            [ordered]@{ name = "$($_.FriendlyName)"; media = "$($_.MediaType)"; health = "$($_.HealthStatus)"; size = [int64]$_.Size
                wear = $rel.Wear; temp = $rel.Temperature; hours = $rel.PowerOnHours }
        })
    @{ disks = $d }
}

# Batería (el informe de powercfg es más fiable que WMI para la capacidad original)
$parts.battery = {
    $ErrorActionPreference = 'SilentlyContinue'
    $bx = Join-Path $env:TEMP "nexo_bateria_$PID.xml"
    powercfg /batteryreport /xml /output $bx | Out-Null
    $bat = $null
    if (Test-Path $bx) { [xml]$bxml = Get-Content -LiteralPath $bx; $bat = $bxml.BatteryReport.Batteries.Battery | Select-Object -First 1; Remove-Item -LiteralPath $bx }
    @{ battery = [ordered]@{ design = [int64]$bat.DesignCapacity; full = [int64]$bat.FullChargeCapacity; cycles = [int64]$bat.CycleCount } }
}

# Microsoft Defender
$parts.defender = {
    $ErrorActionPreference = 'SilentlyContinue'
    $mp = Get-MpComputerStatus
    @{ defender = [ordered]@{
            available = [bool]$mp; enabled = [bool]$mp.AntivirusEnabled; realtime = [bool]$mp.RealTimeProtectionEnabled
            sig_age = [int64]$mp.AntivirusSignatureAge; quick_age = [int64]$mp.QuickScanAge; tamper = [bool]$mp.IsTamperProtected
        }
    }
}

# Firewall
$parts.firewall = {
    $ErrorActionPreference = 'SilentlyContinue'
    @{ firewall = @(Get-NetFirewallProfile | ForEach-Object { [ordered]@{ name = "$($_.Name)"; enabled = [bool]$_.Enabled } }) }
}

# Ajustes del sistema: UAC, escritorio remoto, proxy, hosts, cifrado, tiempo encendido
$parts.system = {
    $ErrorActionPreference = 'SilentlyContinue'
    $o = @{}
    $sys = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System'
    if ($null -ne $sys.EnableLUA) { $o.uac = ($sys.EnableLUA -eq 1) }
    $ts = Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\Terminal Server'
    $o.rdp = ($ts.fDenyTSConnections -eq 0)
    $is = Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings'
    $o.proxy = if ($is.ProxyEnable -eq 1) { "$($is.ProxyServer)" } else { '' }
    $o.hosts = @(Get-Content "$env:WINDIR\System32\drivers\etc\hosts" | Where-Object { $_ -match '^\s*[^#\s]' } | Select-Object -First 10 | ForEach-Object { "$_".Trim() })
    # Cifrado (lectura sin administrador): 1 activo, 2 apagado, 0 no disponible
    $enc = (New-Object -ComObject Shell.Application).NameSpace("$env:SystemDrive\").Self.ExtendedProperty('System.Volume.BitLockerProtection')
    $o.encryption = if ($null -ne $enc) { [int64]$enc } else { -1 }
    $os = Get-CimInstance Win32_OperatingSystem
    $o.uptime_days = [math]::Round(((Get-Date) - $os.LastBootUpTime).TotalDays, 1)
    $o.os = "$($os.Caption) ($($os.Version))"
    $o
}

# Última actualización instalada
$parts.updates = {
    $ErrorActionPreference = 'SilentlyContinue'
    $hf = Get-HotFix | Where-Object { $_.InstalledOn } | Sort-Object InstalledOn -Descending | Select-Object -First 1
    @{ last_update_days = if ($hf) { [int64]((Get-Date) - $hf.InstalledOn).TotalDays } else { -1 } }
}

# Tareas programadas de terceros con señales de riesgo
$parts.tasks = {
    param($bad)
    $ErrorActionPreference = 'SilentlyContinue'
    $tasks = New-Object System.Collections.ArrayList
    $all = @(Get-ScheduledTask | Where-Object { $_.State -ne 'Disabled' -and $_.TaskPath -notlike '\Microsoft\*' })
    foreach ($t in $all) {
        foreach ($a in $t.Actions) {
            if (-not $a.Execute) { continue }
            $exe = [Environment]::ExpandEnvironmentVariables($a.Execute.Trim('"'))
            $arg = "$($a.Arguments)"
            $why = ''
            if ($exe -match $bad) { $why = 'Se ejecuta desde una carpeta temporal o de usuario' }
            elseif ($arg -match '(?i)\s-e(nc|ncodedcommand)?\s+[A-Za-z0-9+/=]{20,}') { $why = 'Ejecuta un comando de PowerShell oculto (codificado)' }
            elseif ($exe -match '(?i)(mshta|wscript|cscript)\.exe$') { $why = 'Usa un intérprete de scripts del sistema' }
            if ($why) { [void]$tasks.Add([ordered]@{ name = $t.TaskName; path = $t.TaskPath; exe = $exe; why = $why }) }
        }
    }
    @{ tasks = @($tasks); tasks_total = $all.Count }
}

# Lanzar todo en paralelo y juntar los resultados.
$pool = [runspacefactory]::CreateRunspacePool(1, $parts.Count)
$pool.ApartmentState = 'STA'
$pool.Open()
$jobs = foreach ($name in $parts.Keys) {
    $ps = [powershell]::Create()
    $ps.RunspacePool = $pool
    [void]$ps.AddScript($parts[$name]).AddArgument($bad)
    [pscustomobject]@{ name = $name; ps = $ps; handle = $ps.BeginInvoke() }
}
# Se recogen a medida que terminan y se avisa a NEXO para su barra de progreso.
$r = [ordered]@{}
$pending = New-Object System.Collections.ArrayList
foreach ($j in $jobs) { [void]$pending.Add($j) }
$total = $pending.Count
$done = 0
while ($pending.Count -gt 0) {
    foreach ($j in @($pending)) {
        if (-not $j.handle.IsCompleted) { continue }
        foreach ($o in $j.ps.EndInvoke($j.handle)) {
            if ($o -is [hashtable]) { foreach ($k in $o.Keys) { $r[$k] = $o[$k] } }
        }
        $j.ps.Dispose()
        [void]$pending.Remove($j)
        $done++
        [Console]::Out.WriteLine("@@PROGRESO $done/$total $($j.name)")
        [Console]::Out.Flush()
    }
    if ($pending.Count -gt 0) { Start-Sleep -Milliseconds 40 }
}
$pool.Close()

$r | ConvertTo-Json -Depth 6 -Compress
