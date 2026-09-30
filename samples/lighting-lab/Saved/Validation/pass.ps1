$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public class Cap {
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
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern bool SetWindowText(IntPtr h, string t);
  [DllImport("user32.dll")] public static extern int GetDlgCtrlID(IntPtr h);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
  [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)]
  public struct TVITEM {
    public uint mask; public IntPtr hItem; public uint state; public uint stateMask;
    public IntPtr pszText; public int cchTextMax; public int iImage; public int iSelectedImage; public int cChildren; public IntPtr lParam;
  }
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr SendMessage(IntPtr h, uint msg, IntPtr w, ref TVITEM item);
  [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h, uint msg, IntPtr w, ref RECT item);
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
  public static string ClassOf(IntPtr h) { var sb = new StringBuilder(256); GetClassName(h, sb, 256); return sb.ToString(); }
  public static string TextOf(IntPtr h) { var sb = new StringBuilder(512); GetWindowText(h, sb, 512); return sb.ToString(); }
  public static List<IntPtr> Descendants(IntPtr parent) {
    var list = new List<IntPtr>();
    EnumChildWindows(parent, (h, l) => { list.Add(h); return true; }, IntPtr.Zero);
    return list;
  }
}
"@
[Cap]::SetProcessDPIAware() | Out-Null
$out = "C:\jarvig\samples\lighting-lab\Saved\Validation\pass"
New-Item -ItemType Directory -Force -Path $out | Out-Null
if (Test-Path "C:\jarvig\samples\lighting-lab\Saved\Editor\viewport.json") { throw "refusing to run while a camera file exists" }
$log = Join-Path $out "stderr.txt"
if (Test-Path $log) { Remove-Item $log -Force }
$proc = Start-Process -FilePath "C:\jarvig\native\target\debug\JARVIGEditor.exe" -WorkingDirectory "C:\jarvig" -RedirectStandardError $log -PassThru
$deadline = (Get-Date).AddSeconds(120)
$text = ""
do {
  Start-Sleep -Seconds 2
  if ($proc.HasExited) { throw "editor exited $($proc.ExitCode)" }
  if (Test-Path $log) { $text = Get-Content $log -Raw -ErrorAction SilentlyContinue }
} while ($text -notmatch "shader compiles" -and (Get-Date) -lt $deadline)
if ($text -notmatch "shader compiles") { throw "ingest did not finish" }
Start-Sleep -Seconds 4
$script:frame = [Cap]::FindClass("JARVIGEditorFrame")
if ($script:frame -eq [IntPtr]::Zero) { throw "frame missing" }
[Cap]::ShowWindow($script:frame, 9) | Out-Null
[Cap]::SetForegroundWindow($script:frame) | Out-Null
Start-Sleep -Milliseconds 500

function Shot([string]$name) {
  $rect = New-Object Cap+RECT
  [Cap]::GetWindowRect($script:frame, [ref]$rect) | Out-Null
  $w = [Math]::Max(1, $rect.R - $rect.L)
  $h = [Math]::Max(1, $rect.B - $rect.T)
  $bmp = New-Object System.Drawing.Bitmap $w, $h
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($rect.L, $rect.T, 0, 0, (New-Object System.Drawing.Size $w, $h))
  $g.Dispose()
  $bmp.Save((Join-Path $out $name), [System.Drawing.Imaging.ImageFormat]::Png)
  $bmp.Dispose()
  Write-Output "SHOT $name"
}
function Viewport-Varied {
  $view = [IntPtr]::Zero
  foreach ($child in [Cap]::Descendants($script:frame)) {
    if ([Cap]::ClassOf($child) -eq "JARVIGEditorViewport") { $view = $child; break }
  }
  if ($view -eq [IntPtr]::Zero) { throw "viewport missing" }
  $rect = New-Object Cap+RECT
  [Cap]::GetWindowRect($view, [ref]$rect) | Out-Null
  $w = [Math]::Max(1, $rect.R - $rect.L)
  $h = [Math]::Max(1, $rect.B - $rect.T)
  $bmp = New-Object System.Drawing.Bitmap $w, $h
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($rect.L, $rect.T, 0, 0, (New-Object System.Drawing.Size $w, $h))
  $g.Dispose()
  $sum = 0.0; $sum2 = 0.0; $n = 0
  $step = 8
  for ($y = [int]($h * 0.2); $y -lt [int]($h * 0.8); $y += $step) {
    for ($x = [int]($w * 0.2); $x -lt [int]($w * 0.8); $x += $step) {
      $c = $bmp.GetPixel($x, $y)
      $l = (0.2126 * $c.R) + (0.7152 * $c.G) + (0.0722 * $c.B)
      $sum += $l; $sum2 += $l * $l; $n++
    }
  }
  $bmp.Dispose()
  $mean = $sum / $n
  $var = ($sum2 / $n) - ($mean * $mean)
  Write-Output ("VIEWPORT stdev {0:N1} mean {1:N1}" -f [Math]::Sqrt([Math]::Max(0, $var)), $mean)
  return [Math]::Sqrt([Math]::Max(0, $var))
}
function Command([int]$id) {
  [Cap]::SendMessage($script:frame, 0x0111, [IntPtr]$id, [IntPtr]::Zero) | Out-Null
  Start-Sleep -Milliseconds 800
}
function Tree {
  foreach ($child in [Cap]::Descendants($script:frame)) {
    if ([Cap]::ClassOf($child) -eq "SysTreeView32") { return $child }
  }
  throw "tree missing"
}
function ItemText($tree, $item) {
  $buf = [Runtime.InteropServices.Marshal]::AllocHGlobal(512)
  $tv = New-Object Cap+TVITEM
  $tv.mask = 1
  $tv.hItem = $item
  $tv.pszText = $buf
  $tv.cchTextMax = 250
  [Cap]::SendMessage($tree, 0x113E, [IntPtr]::Zero, [ref]$tv) | Out-Null
  $text = [Runtime.InteropServices.Marshal]::PtrToStringUni($buf)
  [Runtime.InteropServices.Marshal]::FreeHGlobal($buf)
  return $text
}
function Find-Item($tree, $item, [string]$fragment) {
  while ($item -ne [IntPtr]::Zero) {
    $text = ItemText $tree $item
    if ($text -like "*$fragment*") { return $item }
    $child = [Cap]::SendMessage($tree, 0x110A, [IntPtr]4, $item)
    $found = Find-Item $tree $child $fragment
    if ($found -ne [IntPtr]::Zero) { return $found }
    $item = [Cap]::SendMessage($tree, 0x110A, [IntPtr]1, $item)
  }
  return [IntPtr]::Zero
}
function Click-Row([string]$fragment) {
  $tree = Tree
  $root = [Cap]::SendMessage($tree, 0x110A, [IntPtr]0, [IntPtr]::Zero)
  $item = Find-Item $tree $root $fragment
  if ($item -eq [IntPtr]::Zero) { throw "row not found: $fragment" }
  [Cap]::SendMessage($tree, 0x1114, [IntPtr]::Zero, $item) | Out-Null
  $rect = New-Object Cap+RECT
  $rect.L = $item.ToInt64() -band 0xFFFFFFFF
  # Place the handle in the RECT the way the tree-view expects, then read it back.
  $ptr = [Runtime.InteropServices.Marshal]::AllocHGlobal(32)
  [Runtime.InteropServices.Marshal]::WriteIntPtr($ptr, $item)
  [Cap]::SendMessage($tree, 0x1104, [IntPtr]1, $ptr) | Out-Null
  $left = [Runtime.InteropServices.Marshal]::ReadInt32($ptr, 0)
  $top = [Runtime.InteropServices.Marshal]::ReadInt32($ptr, 4)
  $right = [Runtime.InteropServices.Marshal]::ReadInt32($ptr, 8)
  $bottom = [Runtime.InteropServices.Marshal]::ReadInt32($ptr, 12)
  [Runtime.InteropServices.Marshal]::FreeHGlobal($ptr)
  $pt = New-Object Cap+POINT
  $pt.X = [int](($left + $right) / 2)
  $pt.Y = [int](($top + $bottom) / 2)
  [Cap]::ClientToScreen($tree, [ref]$pt) | Out-Null
  [Cap]::SetForegroundWindow($script:frame) | Out-Null
  [Cap]::SetCursorPos($pt.X, $pt.Y) | Out-Null
  Start-Sleep -Milliseconds 100
  [Cap]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
  [Cap]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds 600
  Write-Output "CLICK $fragment at $($pt.X),$($pt.Y)"
}
function Inspector {
  foreach ($child in [Cap]::Descendants($script:frame)) {
    if ([Cap]::ClassOf($child) -eq "JARVIGInspector") { return $child }
  }
  throw "inspector missing"
}
function Set-Field([string]$label, [string]$value) {
  $inspector = Inspector
  $statics = @(); $edits = @()
  foreach ($child in [Cap]::Descendants($inspector)) {
    $cls = [Cap]::ClassOf($child)
    if ($cls -eq "Static") { $statics += $child }
    if ($cls -eq "Edit") { $edits += $child }
  }
  $labelHwnd = $statics | Where-Object { [Cap]::TextOf($_) -eq $label } | Select-Object -First 1
  if (-not $labelHwnd) { throw "label missing: $label" }
  $lr = New-Object Cap+RECT
  [Cap]::GetWindowRect($labelHwnd, [ref]$lr) | Out-Null
  $best = $null; $bestDy = 9999
  foreach ($edit in $edits) {
    $er = New-Object Cap+RECT
    [Cap]::GetWindowRect($edit, [ref]$er) | Out-Null
    $dy = [Math]::Abs($er.T - $lr.T)
    if ($dy -lt $bestDy) { $bestDy = $dy; $best = $edit }
  }
  if ($bestDy -gt 12) { throw "edit for $label was $bestDy px away" }
  [Cap]::SetWindowText($best, $value) | Out-Null
  $id = [Cap]::GetDlgCtrlID($best)
  $wparam = [IntPtr](([int]0x0200 -shl 16) -bor ($id -band 0xffff))
  [Cap]::SendMessage($inspector, 0x0111, $wparam, $best) | Out-Null
  Start-Sleep -Milliseconds 700
  Write-Output "FIELD $label=$value"
}

$spread = Viewport-Varied
Shot "01-lab.png"
if ($spread -lt 12) { throw "viewport looks blank (stdev $spread). Stopping before any edits." }
Command 1347; Shot "02-base.png"
Command 1348; Shot "03-normal.png"
Command 1349; Shot "04-rough.png"
Command 1350; Shot "05-ao.png"
Command 1351; Shot "06-metal.png"
Command 1346
Command 1311; Shot "07-direct.png"
Command 1312; Shot "08-env.png"
Command 1310; Shot "09-full.png"
Click-Row "Tile Sphere"
Set-Field "Roughness Mult" "0.25"; Shot "10-rough-025.png"
Set-Field "Roughness Mult" "1"; Shot "11-rough-1.png"
Set-Field "Roughness Mult" "2"; Shot "12-rough-2.png"
Set-Field "Roughness Mult" "1"
Set-Field "Normal Strength" "0"; Shot "13-normal-0.png"
Set-Field "Normal Strength" "0.5"; Shot "14-normal-05.png"
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
if (-not $proc.WaitForExit(8000)) { Stop-Process -Id $proc.Id -Force }
Write-Output "CLOSED FOR RELOAD"
