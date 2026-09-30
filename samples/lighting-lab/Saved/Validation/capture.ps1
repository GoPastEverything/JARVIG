$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @"
using System;
using System.Collections.Generic;
using System.Text;
using System.Runtime.InteropServices;
public class Jcap {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc p, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr h, EnumProc p, IntPtr l);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int n);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h, uint msg, IntPtr w, IntPtr l);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern bool SetWindowText(IntPtr h, string t);
  [DllImport("user32.dll")] public static extern int GetDlgCtrlID(IntPtr h);
  public struct RECT { public int L, T, R, B; }
  public static IntPtr FindClass(string cls) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, l) => {
      var sb = new StringBuilder(256);
      GetClassName(h, sb, 256);
      if (sb.ToString() == cls) { found = h; return false; }
      return true;
    }, IntPtr.Zero);
    return found;
  }
  public static List<IntPtr> Children(IntPtr parent) {
    var list = new List<IntPtr>();
    EnumChildWindows(parent, (h, l) => { list.Add(h); return true; }, IntPtr.Zero);
    return list;
  }
  public static string ClassOf(IntPtr h) {
    var sb = new StringBuilder(256);
    GetClassName(h, sb, 256);
    return sb.ToString();
  }
  public static string TextOf(IntPtr h) {
    var sb = new StringBuilder(512);
    GetWindowText(h, sb, 512);
    return sb.ToString();
  }
}
"@
[Jcap]::SetProcessDPIAware() | Out-Null
$root = "C:\jarvig\samples\lighting-lab\Saved\Validation"
New-Item -ItemType Directory -Force -Path $root | Out-Null
$log = Join-Path $root "editor-stderr.txt"
if (Test-Path $log) { Remove-Item $log -Force }
$exe = "C:\jarvig\native\target\debug\JARVIGEditor.exe"
$proc = Start-Process -FilePath $exe -WorkingDirectory "C:\jarvig" -RedirectStandardError $log -PassThru
$deadline = (Get-Date).AddSeconds(120)
do {
  Start-Sleep -Seconds 2
  if ($proc.HasExited) { throw "editor exited early: $($proc.ExitCode)" }
  $text = if (Test-Path $log) { Get-Content $log -Raw -ErrorAction SilentlyContinue } else { "" }
} while ($text -notmatch "shader compiles" -and (Get-Date) -lt $deadline)
if ($text -notmatch "shader compiles") { throw "material ingest did not finish" }
Start-Sleep -Seconds 3
$frame = [Jcap]::FindClass("JARVIGEditorFrame")
if ($frame -eq [IntPtr]::Zero) { throw "editor frame was not found" }
[Jcap]::ShowWindow($frame, 9) | Out-Null
[Jcap]::SetForegroundWindow($frame) | Out-Null
Start-Sleep -Milliseconds 400

function Shot([string]$name) {
  $rect = New-Object Jcap+RECT
  [Jcap]::GetWindowRect($frame, [ref]$rect) | Out-Null
  $w = [Math]::Max(1, $rect.R - $rect.L)
  $h = [Math]::Max(1, $rect.B - $rect.T)
  $bmp = New-Object System.Drawing.Bitmap $w, $h
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($rect.L, $rect.T, 0, 0, (New-Object System.Drawing.Size $w, $h))
  $path = Join-Path $root $name
  $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
  Write-Output "SHOT $name ${w}x${h}"
}
function Command([int]$id) {
  [Jcap]::SendMessage($frame, 0x0111, [IntPtr]$id, [IntPtr]::Zero) | Out-Null
  Start-Sleep -Milliseconds 700
}
function Select-Row([string]$fragment) {
  $rootEl = [System.Windows.Automation.AutomationElement]::FromHandle($frame)
  $all = $rootEl.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
  foreach ($el in $all) {
    $name = ""
    try { $name = $el.Current.Name } catch {}
    if ($name -like "*$fragment*") {
      try {
        $pattern = $el.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern)
        $pattern.Select()
        Start-Sleep -Milliseconds 500
        Write-Output "SELECTED $name"
        return
      } catch {}
    }
  }
  throw "could not select $fragment"
}
function Set-Field([string]$label, [string]$value) {
  $inspector = [IntPtr]::Zero
  foreach ($child in [Jcap]::Children($frame)) {
    if ([Jcap]::ClassOf($child) -eq "JARVIGInspector") { $inspector = $child; break }
    foreach ($grand in [Jcap]::Children($child)) {
      if ([Jcap]::ClassOf($grand) -eq "JARVIGInspector") { $inspector = $grand; break }
    }
    if ($inspector -ne [IntPtr]::Zero) { break }
  }
  if ($inspector -eq [IntPtr]::Zero) { throw "inspector missing" }
  $statics = @()
  $edits = @()
  foreach ($child in [Jcap]::Children($inspector)) {
    $cls = [Jcap]::ClassOf($child)
    if ($cls -eq "Static") { $statics += $child }
    if ($cls -eq "Edit") { $edits += $child }
  }
  $labelHwnd = $statics | Where-Object { [Jcap]::TextOf($_) -eq $label } | Select-Object -First 1
  if (-not $labelHwnd) { throw "label missing: $label" }
  $lr = New-Object Jcap+RECT
  [Jcap]::GetWindowRect($labelHwnd, [ref]$lr) | Out-Null
  $best = $null
  $bestDy = 9999
  foreach ($edit in $edits) {
    $er = New-Object Jcap+RECT
    [Jcap]::GetWindowRect($edit, [ref]$er) | Out-Null
    $dy = [Math]::Abs($er.T - $lr.T)
    if ($dy -lt $bestDy) { $bestDy = $dy; $best = $edit }
  }
  if ($bestDy -gt 8) { throw "edit for $label was $bestDy px away" }
  [Jcap]::SetWindowText($best, $value) | Out-Null
  $id = [Jcap]::GetDlgCtrlID($best)
  $wparam = [IntPtr](([int]0x0200 -shl 16) -bor ($id -band 0xffff))
  [Jcap]::SendMessage($inspector, 0x0111, $wparam, $best) | Out-Null
  Start-Sleep -Milliseconds 800
  Write-Output "FIELD $label=$value"
}

Shot "01-uv-0.5-1-2-4.png"
Command 1347; Shot "02-base-color.png"
Command 1348; Shot "03-normal.png"
Command 1349; Shot "04-roughness.png"
Command 1350; Shot "05-ao.png"
Command 1351; Shot "06-metallic.png"
Command 1346
Command 1311; Shot "07-direct-only.png"
Command 1312; Shot "08-env-diffuse.png"
Command 1310; Shot "09-full-lighting.png"
Select-Row "Tile Sphere"
Set-Field "Roughness Mult" "0.25"; Shot "10-roughness-0.25.png"
Set-Field "Roughness Mult" "1"; Shot "11-roughness-1.png"
Set-Field "Roughness Mult" "2"; Shot "12-roughness-2.png"
Set-Field "Normal Strength" "0"; Shot "13-normal-0.png"
Set-Field "Normal Strength" "0.5"; Shot "14-normal-0.5.png"
Set-Field "Normal Strength" "1"; Shot "15-normal-1.png"
Set-Field "Normal Strength" "2"; Shot "16-normal-2.png"
Set-Field "UV Scale" "3"
Set-Field "Roughness Mult" "0.25"
Set-Field "Metallic Mult" "0.4"
Set-Field "Normal Strength" "2"
Command 1015
Start-Sleep -Seconds 1
Write-Output "SAVED"
$proc.CloseMainWindow() | Out-Null
$ok = $proc.WaitForExit(8000)
if (-not $ok) { Stop-Process -Id $proc.Id -Force }
Write-Output "EDITOR CLOSED"
