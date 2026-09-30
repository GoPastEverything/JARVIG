$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public class Fac {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc p, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr h, EnumProc p, IntPtr l);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int n);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
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
    public IntPtr pszText; public int cchTextMax; public int iImage; public int iSelectedImage;
    public int cChildren; public IntPtr lParam;
  }
  [DllImport("user32.dll", EntryPoint="SendMessageW", CharSet=CharSet.Unicode)]
  public static extern IntPtr SendItem(IntPtr h, uint msg, IntPtr w, ref TVITEM item);
  public static IntPtr Frame() {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, l) => {
      var sb = new StringBuilder(256);
      GetClassName(h, sb, 256);
      if (sb.ToString() == "JARVIGEditorFrame") { found = h; return false; }
      return true;
    }, IntPtr.Zero);
    return found;
  }
  public static string ClassOf(IntPtr h) { var sb = new StringBuilder(256); GetClassName(h, sb, 256); return sb.ToString(); }
  public static string TextOf(IntPtr h) { var sb = new StringBuilder(512); GetWindowText(h, sb, 512); return sb.ToString(); }
  public static List<IntPtr> Kids(IntPtr parent) {
    var list = new List<IntPtr>();
    EnumChildWindows(parent, (h, l) => { list.Add(h); return true; }, IntPtr.Zero);
    return list;
  }
  public static string ItemText(IntPtr tree, IntPtr item) {
    IntPtr buf = Marshal.AllocHGlobal(512);
    var tv = new TVITEM();
    tv.mask = 1;
    tv.hItem = item;
    tv.pszText = buf;
    tv.cchTextMax = 250;
    SendItem(tree, 0x113E, IntPtr.Zero, ref tv);
    string text = Marshal.PtrToStringUni(buf) ?? "";
    Marshal.FreeHGlobal(buf);
    return text;
  }
  public static IntPtr FindRow(IntPtr tree, IntPtr item, string fragment) {
    while (item != IntPtr.Zero) {
      if (ItemText(tree, item).IndexOf(fragment, StringComparison.OrdinalIgnoreCase) >= 0) return item;
      IntPtr child = SendMessage(tree, 0x110A, (IntPtr)4, item);
      IntPtr found = FindRow(tree, child, fragment);
      if (found != IntPtr.Zero) return found;
      item = SendMessage(tree, 0x110A, (IntPtr)1, item);
    }
    return IntPtr.Zero;
  }
}
"@
[Fac]::SetProcessDPIAware() | Out-Null
if (Test-Path "C:\jarvig\samples\lighting-lab\Saved\Editor\viewport.json") { throw "camera file exists" }
$out = "C:\jarvig\samples\lighting-lab\Saved\Validation\pass"
New-Item -ItemType Directory -Force -Path $out | Out-Null
$existing = Get-Process JARVIGEditor -ErrorAction SilentlyContinue
if ($existing) {
  $proc = $existing | Select-Object -First 1
  Write-Output "ATTACH $($proc.Id)"
} else {
  $proc = Start-Process -FilePath "C:\jarvig\native\target\debug\JARVIGEditor.exe" -WorkingDirectory "C:\jarvig" -PassThru
  $deadline = (Get-Date).AddSeconds(120)
  $title = ""
  do {
    Start-Sleep -Seconds 2
    if ($proc.HasExited) { throw "editor exited $($proc.ExitCode)" }
    $script:frame = [Fac]::Frame()
    if ($script:frame -ne [IntPtr]::Zero) { $title = [Fac]::TextOf($script:frame) }
  } while ($title -notmatch "Lighting Lab" -and (Get-Date) -lt $deadline)
  if ($title -notmatch "Lighting Lab") { throw "lab title did not appear: $title" }
  Start-Sleep -Seconds 6
}
$script:frame = [Fac]::Frame()
if ($script:frame -eq [IntPtr]::Zero) { throw "frame missing" }
[Fac]::ShowWindow($script:frame, 9) | Out-Null
[Fac]::SetWindowPos($script:frame, [IntPtr](-1), 0, 0, 0, 0, 0x0003) | Out-Null
[Fac]::SetForegroundWindow($script:frame) | Out-Null
Start-Sleep -Seconds 1
Write-Output ("TITLE " + [Fac]::TextOf($script:frame))

function Shot([string]$name) {
  $rect = New-Object Fac+RECT
  [Fac]::GetWindowRect($script:frame, [ref]$rect) | Out-Null
  $w = [Math]::Max(1, $rect.R - $rect.L); $h = [Math]::Max(1, $rect.B - $rect.T)
  $bmp = New-Object System.Drawing.Bitmap $w, $h
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($rect.L, $rect.T, 0, 0, (New-Object System.Drawing.Size $w, $h))
  $g.Dispose()
  $bmp.Save((Join-Path $out $name), [System.Drawing.Imaging.ImageFormat]::Png)
  $bmp.Dispose()
  Write-Output "SHOT $name ${w}x${h}"
}
function Command([int]$id) {
  [Fac]::SendMessage($script:frame, 0x0111, [IntPtr]$id, [IntPtr]::Zero) | Out-Null
  Start-Sleep -Milliseconds 1200
}
function Click-Row([string]$fragment) {
  $tree = [IntPtr]::Zero
  foreach ($child in [Fac]::Kids($script:frame)) {
    if ([Fac]::ClassOf($child) -eq "SysTreeView32") { $tree = $child; break }
  }
  if ($tree -eq [IntPtr]::Zero) { throw "tree missing" }
  $root = [Fac]::SendMessage($tree, 0x110A, [IntPtr]0, [IntPtr]::Zero)
  $item = [Fac]::FindRow($tree, $root, $fragment)
  if ($item -eq [IntPtr]::Zero) {
    $sample = [Fac]::ItemText($tree, $root)
    throw "row not found: $fragment (root text '$sample')"
  }
  [Fac]::SendMessage($tree, 0x1114, [IntPtr]::Zero, $item) | Out-Null
  $ptr = [Runtime.InteropServices.Marshal]::AllocHGlobal(32)
  [Runtime.InteropServices.Marshal]::WriteIntPtr($ptr, $item)
  [Fac]::SendMessage($tree, 0x1104, [IntPtr]1, $ptr) | Out-Null
  $left = [Runtime.InteropServices.Marshal]::ReadInt32($ptr, 0)
  $top = [Runtime.InteropServices.Marshal]::ReadInt32($ptr, 4)
  $right = [Runtime.InteropServices.Marshal]::ReadInt32($ptr, 8)
  $bottom = [Runtime.InteropServices.Marshal]::ReadInt32($ptr, 12)
  [Runtime.InteropServices.Marshal]::FreeHGlobal($ptr)
  $pt = New-Object Fac+POINT
  $pt.X = [int](($left + $right) / 2)
  $pt.Y = [int](($top + $bottom) / 2)
  [Fac]::ClientToScreen($tree, [ref]$pt) | Out-Null
  [Fac]::SetCursorPos($pt.X, $pt.Y) | Out-Null
  Start-Sleep -Milliseconds 80
  [Fac]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
  [Fac]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds 700
  Write-Output "CLICK $fragment $($pt.X),$($pt.Y) text=$([Fac]::ItemText($tree, $item))"
}
function Set-Field([string]$label, [string]$value) {
  $inspector = [IntPtr]::Zero
  foreach ($child in [Fac]::Kids($script:frame)) {
    if ([Fac]::ClassOf($child) -eq "JARVIGInspector") { $inspector = $child; break }
  }
  if ($inspector -eq [IntPtr]::Zero) { throw "inspector missing" }
  $target = [IntPtr]::Zero
  $best = 9999
  $labelRect = New-Object Fac+RECT
  $foundLabel = $false
  foreach ($child in [Fac]::Kids($inspector)) {
    if ([Fac]::ClassOf($child) -eq "Static" -and [Fac]::TextOf($child) -eq $label) {
      [Fac]::GetWindowRect($child, [ref]$labelRect) | Out-Null
      $foundLabel = $true
      break
    }
  }
  if (-not $foundLabel) { throw "label missing $label" }
  foreach ($child in [Fac]::Kids($inspector)) {
    if ([Fac]::ClassOf($child) -ne "Edit") { continue }
    $er = New-Object Fac+RECT
    [Fac]::GetWindowRect($child, [ref]$er) | Out-Null
    $dy = [Math]::Abs($er.T - $labelRect.T)
    if ($dy -lt $best) { $best = $dy; $target = $child }
  }
  if ($best -gt 12) { throw "edit for $label was $best px away" }
  [Fac]::SetWindowText($target, $value) | Out-Null
  $id = [Fac]::GetDlgCtrlID($target)
  $wparam = [IntPtr]((0x0200 -shl 16) -bor ($id -band 0xffff))
  [Fac]::SendMessage($inspector, 0x0111, $wparam, $target) | Out-Null
  Start-Sleep -Milliseconds 900
  Write-Output "FIELD $label=$value"
}

Command 1346
Command 1310
Shot "20-lab.png"
Command 1347; Shot "21-base.png"; Command 1346
Click-Row "Tile Sphere"
Set-Field "Roughness Mult" "0.25"; Shot "22-rough-025.png"
Set-Field "Roughness Mult" "1"; Shot "23-rough-1.png"
Set-Field "Roughness Mult" "2"; Shot "24-rough-2.png"
Set-Field "Roughness Mult" "1"
Set-Field "Normal Strength" "0"; Shot "25-normal-0.png"
Set-Field "Normal Strength" "0.5"; Shot "26-normal-05.png"
Set-Field "Normal Strength" "1"; Shot "27-normal-1.png"
Set-Field "Normal Strength" "2"; Shot "28-normal-2.png"
Set-Field "UV Scale" "3"
Set-Field "Roughness Mult" "0.25"
Set-Field "Metallic Mult" "0.4"
Set-Field "Normal Strength" "2"
Command 1015
Start-Sleep -Seconds 1
Write-Output "SAVED"
[Fac]::SetWindowPos($script:frame, [IntPtr](-2), 0, 0, 0, 0, 0x0003) | Out-Null
$proc.CloseMainWindow() | Out-Null
if (-not $proc.WaitForExit(8000)) { Stop-Process -Id $proc.Id -Force }
Write-Output "CLOSED"
