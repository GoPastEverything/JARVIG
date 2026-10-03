# Build the local JARVIG documentation site from Markdown.
# Output: docs/site/
# That folder is gitignored. Do not copy it onto the public tree.
# Session logs, benchmark dumps, and the private surface-rule pages stay out of the site.
# Requires Windows PowerShell 5.1 or pwsh. ASCII on purpose so Windows PowerShell parses it.

$ErrorActionPreference = 'Stop'
$utf8 = New-Object System.Text.UTF8Encoding $false

$repo = Split-Path -Parent $PSScriptRoot
$docsRoot = Join-Path $repo 'docs'
$site = Join-Path $docsRoot 'site'
$resolvedSite = [System.IO.Path]::GetFullPath($site)
$resolvedRepo = [System.IO.Path]::GetFullPath($repo)
if (-not $resolvedSite.StartsWith($resolvedRepo, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to write outside the repository: $resolvedSite"
}
if (-not $resolvedSite.EndsWith([System.IO.Path]::DirectorySeparatorChar + 'docs' + [System.IO.Path]::DirectorySeparatorChar + 'site', [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to wipe an unexpected path: $resolvedSite"
}
if (Test-Path -LiteralPath $site) {
    Remove-Item -LiteralPath $site -Recurse -Force
}
New-Item -ItemType Directory -Path $site | Out-Null
New-Item -ItemType Directory -Path (Join-Path $site 'assets') | Out-Null

function Encode-Html([string]$Text) {
    if ([string]::IsNullOrEmpty($Text)) { return '' }
    return [System.Net.WebUtility]::HtmlEncode($Text)
}

function Encode-Json([string]$Text) {
    if ($null -eq $Text) { $Text = '' }
    $Text = $Text.Replace('\', '\\').Replace('"', '\"').Replace("`r", '').Replace("`n", '\n').Replace("`t", '\t')
    return $Text
}

function Normalize-RepoPath([string]$Path) {
    $parts = New-Object System.Collections.Generic.List[string]
    foreach ($piece in ($Path -replace '\\', '/' -split '/')) {
        if ($piece -eq '' -or $piece -eq '.') { continue }
        if ($piece -eq '..') {
            if ($parts.Count -gt 0) { $parts.RemoveAt($parts.Count - 1) }
            continue
        }
        $parts.Add($piece)
    }
    return ($parts -join '/')
}

function Test-ExcludedDoc([string]$RepoRel) {
    $path = $RepoRel -replace '\\', '/'
    if ($path -eq 'docs/BACKLOG.md') { return $true }
    if ($path -like 'docs/status' -or $path -like 'docs/status/*') { return $true }
    if ($path -like 'docs/benchmarks' -or $path -like 'docs/benchmarks/*') { return $true }
    if ($path -eq 'docs/rendering/microgeometry.md') { return $true }
    if ($path -eq 'docs/research/aperiodic-detail.md') { return $true }
    if ($path -like 'docs/rfc/RFC-0002*') { return $true }
    if ($path -like 'private/*' -or $path -like '*/private/*') { return $true }
    return $false
}

function Get-RelHref([string]$FromOutput, [string]$ToOutput) {
    $fromDir = [System.IO.Path]::GetDirectoryName(($FromOutput -replace '/', '\'))
    $prefix = ''
    if (-not [string]::IsNullOrEmpty($fromDir)) {
        $prefix = ($fromDir -replace '\\', '/') + '/'
    }
    $baseUri = New-Object System.Uri ("http://site/$prefix")
    $toUri = New-Object System.Uri ("http://site/$ToOutput")
    return [System.Uri]::UnescapeDataString($baseUri.MakeRelativeUri($toUri).ToString())
}

function Get-Slug([string]$Title, $Used) {
    $slug = $Title.ToLowerInvariant() -replace '[^a-z0-9\s-]', ''
    $slug = ($slug -replace '\s+', '-').Trim('-')
    if (-not $slug) { $slug = 'section' }
    $base = $slug
    $n = 2
    while ($Used.ContainsKey($slug)) {
        $slug = "$base-$n"
        $n++
    }
    $Used[$slug] = $true
    return $slug
}

function Get-DocTitle([string]$Markdown, [string]$Fallback) {
    foreach ($line in ($Markdown -split '\r?\n')) {
        if ($line -match '^#\s+(.+?)\s*$') {
            return ($Matches[1] -replace '`', '')
        }
    }
    return $Fallback
}

$script:Pages = @{}
$script:Media = @{}

function Copy-MediaFile([string]$RepoRel) {
    if ($script:Media.ContainsKey($RepoRel)) { return $script:Media[$RepoRel] }
    if (Test-ExcludedDoc $RepoRel) { return $null }
    if ($RepoRel -match '^[A-Za-z]:' -or $RepoRel.StartsWith('//')) { return $null }
    $src = Join-Path $repo ($RepoRel -replace '/', '\')
    if (-not (Test-Path -LiteralPath $src -PathType Leaf)) { return $null }
    $ext = [System.IO.Path]::GetExtension($src).ToLowerInvariant()
    if (@('.png', '.jpg', '.jpeg', '.gif', '.svg', '.webp') -notcontains $ext) { return $null }
    if ((Get-Item -LiteralPath $src).Length -gt 5MB) { return $null }
    $destRel = '_media/' + $RepoRel
    $dest = Join-Path $site ($destRel -replace '/', '\')
    $destDir = Split-Path -Parent $dest
    if (-not (Test-Path -LiteralPath $destDir)) {
        New-Item -ItemType Directory -Path $destDir -Force | Out-Null
    }
    Copy-Item -LiteralPath $src -Destination $dest
    $script:Media[$RepoRel] = $destRel
    return $destRel
}

function Convert-Inline {
    param(
        [string]$Text,
        $Ctx,
        [switch]$NoLinks
    )
    if ($null -eq $Text) { return '' }
    $sb = New-Object System.Text.StringBuilder
    $i = 0
    $n = $Text.Length
    while ($i -lt $n) {
        $ch = $Text[$i]
        if ($ch -eq '`') {
            $j = $Text.IndexOf([char]'`', $i + 1)
            if ($j -gt $i) {
                [void]$sb.Append('<code>')
                [void]$sb.Append((Encode-Html $Text.Substring($i + 1, $j - $i - 1)))
                [void]$sb.Append('</code>')
                $i = $j + 1
                continue
            }
        }
        if (-not $NoLinks -and $ch -eq '!' -and ($i + 1) -lt $n -and $Text[$i + 1] -eq '[') {
            $close = $Text.IndexOf(']', $i + 2)
            if ($close -gt $i -and ($close + 1) -lt $n -and $Text[$close + 1] -eq '(') {
                $end = $Text.IndexOf(')', $close + 2)
                if ($end -gt $close) {
                    $alt = $Text.Substring($i + 2, $close - ($i + 2))
                    $url = $Text.Substring($close + 2, $end - ($close + 2)).Trim()
                    [void]$sb.Append((Format-Image $alt $url $Ctx))
                    $i = $end + 1
                    continue
                }
            }
        }
        if (-not $NoLinks -and $ch -eq '[') {
            $close = $Text.IndexOf(']', $i + 1)
            if ($close -gt $i -and ($close + 1) -lt $n -and $Text[$close + 1] -eq '(') {
                $end = $Text.IndexOf(')', $close + 2)
                if ($end -gt $close) {
                    $label = $Text.Substring($i + 1, $close - $i - 1)
                    $url = $Text.Substring($close + 2, $end - ($close + 2)).Trim()
                    [void]$sb.Append((Format-Anchor $label $url $Ctx))
                    $i = $end + 1
                    continue
                }
            }
        }
        if ($ch -eq '*' -and ($i + 1) -lt $n -and $Text[$i + 1] -eq '*') {
            $j = $Text.IndexOf('**', $i + 2)
            if ($j -gt $i) {
                [void]$sb.Append('<strong>')
                [void]$sb.Append((Convert-Inline $Text.Substring($i + 2, $j - ($i + 2)) $Ctx))
                [void]$sb.Append('</strong>')
                $i = $j + 2
                continue
            }
        }
        [void]$sb.Append((Encode-Html ([string]$ch)))
        $i++
    }
    return $sb.ToString()
}

function Resolve-RepoTarget([string]$Source, [string]$Url) {
    $hash = ''
    $path = $Url.Trim()
    $space = $path.IndexOf(' ')
    if ($space -gt 0 -and $path.Substring($space).Trim().StartsWith('"')) {
        $path = $path.Substring(0, $space)
    }
    $hi = $path.IndexOf('#')
    if ($hi -ge 0) {
        $hash = $path.Substring($hi)
        $path = $path.Substring(0, $hi)
    }
    if (-not $path) { return @{ Kind = 'hash'; Hash = $hash } }
    if ($path -match '^(https?:|mailto:)') { return @{ Kind = 'external'; Href = $Url.Trim() } }
    if ($path -match '^[A-Za-z][A-Za-z0-9+.\-]*:') { return @{ Kind = 'external'; Href = $Url.Trim() } }
    $sourceDir = [System.IO.Path]::GetDirectoryName(($Source -replace '/', '\'))
    if ([string]::IsNullOrEmpty($sourceDir)) { $joined = $path } else { $joined = (($sourceDir -replace '\\', '/') + '/' + $path) }
    $joined = Normalize-RepoPath $joined
    return @{ Kind = 'repo'; Path = $joined; Hash = $hash }
}

function Format-Anchor([string]$Label, [string]$Url, $Ctx) {
    $inner = Convert-Inline $Label $Ctx -NoLinks
    $target = Resolve-RepoTarget $Ctx.Source $Url
    if ($target.Kind -eq 'external') {
        return '<a href="' + (Encode-Html $target.Href) + '">' + $inner + '</a>'
    }
    if ($target.Kind -eq 'hash') {
        return '<a href="' + (Encode-Html $target.Hash) + '">' + $inner + '</a>'
    }
    if (Test-ExcludedDoc $target.Path) {
        return '<span class="repo-only" title="Kept in the repository. This site does not publish that page.">' + $inner + '</span>'
    }
    if ($script:Pages.ContainsKey($target.Path)) {
        $href = (Get-RelHref $Ctx.Output $script:Pages[$target.Path]) + $target.Hash
        return '<a href="' + (Encode-Html $href) + '">' + $inner + '</a>'
    }
    return '<span class="repo-only" title="Kept in the repository. This site does not publish that page.">' + $inner + '</span>'
}

function Format-Image([string]$Alt, [string]$Url, $Ctx) {
    $alt = Encode-Html $Alt
    $target = Resolve-RepoTarget $Ctx.Source $Url
    if ($target.Kind -eq 'external') {
        return '<img alt="' + $alt + '" src="' + (Encode-Html $target.Href) + '">'
    }
    if ($target.Kind -eq 'repo') {
        $copied = Copy-MediaFile $target.Path
        if ($copied) {
            $href = Get-RelHref $Ctx.Output $copied
            return '<img alt="' + $alt + '" src="' + (Encode-Html $href) + '">'
        }
    }
    return '<span class="repo-only">' + $alt + '</span>'
}

function Split-TableRow([string]$Line) {
    $trim = $Line.Trim()
    if ($trim.StartsWith('|')) { $trim = $trim.Substring(1) }
    if ($trim.EndsWith('|')) { $trim = $trim.Substring(0, $trim.Length - 1) }
    return @($trim -split '\|' | ForEach-Object { $_.Trim() })
}

function Convert-Markdown([string]$Markdown, $Ctx) {
    $lines = $Markdown -split '\r?\n', -1
    $out = New-Object System.Text.StringBuilder
    $toc = New-Object System.Collections.Generic.List[object]
    $used = @{}
    $i = 0
    while ($i -lt $lines.Count) {
        $line = $lines[$i]
        if ($line -match '^\s*```') {
            $i++
            $code = New-Object System.Collections.Generic.List[string]
            while ($i -lt $lines.Count -and $lines[$i] -notmatch '^\s*```') {
                $code.Add($lines[$i])
                $i++
            }
            if ($i -lt $lines.Count) { $i++ }
            [void]$out.Append('<pre><code>')
            [void]$out.Append((Encode-Html ($code -join "`n")))
            [void]$out.Append("</code></pre>`n")
            continue
        }
        if ($line -match '^\s*$' -or $line -match '^\s*<!--.*-->\s*$') {
            $i++
            continue
        }
        if ($line -match '^(#{1,6})\s+(.*?)\s*$') {
            $level = $Matches[1].Length
            $title = $Matches[2]
            $id = Get-Slug $title $used
            if ($level -ge 2 -and $level -le 3) {
                $toc.Add([pscustomobject]@{ Level = $level; Id = $id; Text = ($title -replace '`', '') })
            }
            [void]$out.Append("<h$level id=`"$id`">")
            [void]$out.Append((Convert-Inline $title $Ctx))
            [void]$out.Append("</h$level>`n")
            $i++
            continue
        }
        if ($line -match '^(\*\s*\*\s*\*|-{3,}|_{3,})\s*$') {
            [void]$out.Append("<hr>`n")
            $i++
            continue
        }
        if ($line -match '^\s*\|') {
            $rows = New-Object System.Collections.Generic.List[string]
            while ($i -lt $lines.Count -and $lines[$i] -match '^\s*\|') {
                $rows.Add($lines[$i])
                $i++
            }
            if ($rows.Count -ge 2) {
                $header = Split-TableRow $rows[0]
                $start = 1
                if ($rows[1] -match '^[\s\|:\-]+$') { $start = 2 }
                [void]$out.Append("<table><thead><tr>")
                foreach ($cell in $header) {
                    [void]$out.Append('<th>')
                    [void]$out.Append((Convert-Inline $cell $Ctx))
                    [void]$out.Append('</th>')
                }
                [void]$out.Append('</tr></thead><tbody>')
                for ($r = $start; $r -lt $rows.Count; $r++) {
                    [void]$out.Append('<tr>')
                    foreach ($cell in (Split-TableRow $rows[$r])) {
                        [void]$out.Append('<td>')
                        [void]$out.Append((Convert-Inline $cell $Ctx))
                        [void]$out.Append('</td>')
                    }
                    [void]$out.Append('</tr>')
                }
                [void]$out.Append("</tbody></table>`n")
            }
            continue
        }
        if ($line -match '^\s*([-*]|\d+\.)\s+') {
            $ordered = $line -match '^\s*\d+\.\s+'
            $tag = if ($ordered) { 'ol' } else { 'ul' }
            [void]$out.Append("<$tag>`n")
            while ($i -lt $lines.Count -and $lines[$i] -match '^\s*([-*]|\d+\.)\s+(.*)$') {
                [void]$out.Append('<li>')
                [void]$out.Append((Convert-Inline $Matches[2] $Ctx))
                [void]$out.Append("</li>`n")
                $i++
            }
            [void]$out.Append("</$tag>`n")
            continue
        }
        if ($line -match '^>\s?(.*)$') {
            $quote = New-Object System.Collections.Generic.List[string]
            while ($i -lt $lines.Count -and $lines[$i] -match '^>\s?(.*)$') {
                $quote.Add($Matches[1])
                $i++
            }
            [void]$out.Append('<blockquote><p>')
            [void]$out.Append((Convert-Inline ($quote -join ' ') $Ctx))
            [void]$out.Append("</p></blockquote>`n")
            continue
        }
        $para = New-Object System.Collections.Generic.List[string]
        while ($i -lt $lines.Count -and $lines[$i] -notmatch '^\s*$' -and $lines[$i] -notmatch '^(#{1,6}\s|>\s?|\s*```|\s*\|)' -and $lines[$i] -notmatch '^\s*([-*]|\d+\.)\s+' -and $lines[$i] -notmatch '^(\*\s*\*\s*\*|-{3,}|_{3,})\s*$') {
            $para.Add($lines[$i].Trim())
            $i++
        }
        if ($para.Count -gt 0) {
            [void]$out.Append('<p>')
            [void]$out.Append((Convert-Inline ($para -join ' ') $Ctx))
            [void]$out.Append("</p>`n")
        }
    }
    return [pscustomobject]@{ Html = $out.ToString(); Toc = $toc }
}

$allowDirs = @(
    'manual', 'editor', 'hub', 'architecture', 'api', 'adr', 'rendering', 'materials', 'terrain', 'assets',
    'animation', 'physics', 'ai', 'audio', 'networking', 'scripting', 'plugins', 'build', 'cli', 'profiling',
    'testing', 'platform', 'legal', 'rfc', 'research'
)
$groupNames = @{
    'editor' = 'Editor'
    'hub' = 'Hub'
    'architecture' = 'Architecture'
    'api' = 'API'
    'adr' = 'Decisions'
    'rendering' = 'Rendering'
    'materials' = 'Materials'
    'terrain' = 'Terrain'
    'assets' = 'Assets'
    'animation' = 'Animation'
    'physics' = 'Physics'
    'ai' = 'AI'
    'audio' = 'Audio'
    'networking' = 'Networking'
    'scripting' = 'Scripting'
    'plugins' = 'Plugins'
    'build' = 'Build'
    'cli' = 'Command line'
    'profiling' = 'Profiling'
    'testing' = 'Testing'
    'platform' = 'Platform'
    'legal' = 'Legal'
    'rfc' = 'Proposals'
    'research' = 'Research'
}
$startOrder = @(
    'docs/manual/README.md',
    'docs/getting-started.md',
    'docs/manual/editor.md',
    'docs/manual/modeling.md',
    'docs/manual/content.md',
    'docs/manual/worlds.md',
    'docs/manual/lighting.md',
    'docs/manual/play.md',
    'docs/manual/programming.md',
    'docs/manual/coordinates.md',
    'docs/manual/limits.md',
    'docs/manual/authority.md',
    'docs/README.md'
)
$rootFiles = @('README.md', 'ROADMAP.md', 'ARCHITECTURE.md', 'AGENTS.md', 'LICENSES.md')

$found = New-Object System.Collections.Generic.List[string]
foreach ($dir in $allowDirs) {
    $full = Join-Path $docsRoot $dir
    if (-not (Test-Path -LiteralPath $full)) { continue }
    Get-ChildItem -LiteralPath $full -Recurse -Filter *.md -File | ForEach-Object {
        $rel = $_.FullName.Substring($docsRoot.Length).TrimStart('\') -replace '\\', '/'
        $repoRel = "docs/$rel"
        if (-not (Test-ExcludedDoc $repoRel)) { $found.Add($repoRel) }
    }
}
foreach ($extra in @('docs/README.md', 'docs/getting-started.md')) {
    $full = Join-Path $repo ($extra -replace '/', '\')
    if ((Test-Path -LiteralPath $full) -and -not $found.Contains($extra)) { $found.Add($extra) }
}
foreach ($rootName in $rootFiles) {
    $full = Join-Path $repo $rootName
    if (Test-Path -LiteralPath $full) { $found.Add($rootName) }
}

$catalog = @()
foreach ($source in $found) {
    if ($source -like 'docs/*') {
        $output = ($source.Substring(5) -replace '\.md$', '.html')
    } else {
        $output = 'repo/' + ($source -replace '\.md$', '.html')
    }
    $script:Pages[$source] = $output
    $rawPath = Join-Path $repo ($source -replace '/', '\')
    $raw = [System.IO.File]::ReadAllText($rawPath)
    $fallback = [System.IO.Path]::GetFileNameWithoutExtension($source)
    $title = Get-DocTitle $raw $fallback
    $catalog += [pscustomobject]@{ Source = $source; Output = $output; Title = $title; Raw = $raw }
}

$bySource = @{}
foreach ($page in $catalog) { $bySource[$page.Source] = $page }

$groups = New-Object System.Collections.Generic.List[object]
$startItems = New-Object System.Collections.Generic.List[object]
foreach ($source in $startOrder) {
    if ($bySource.ContainsKey($source)) {
        $page = $bySource[$source]
        $startItems.Add($page)
    }
}
$groups.Add([pscustomobject]@{ Name = 'Start'; Key = 'start'; Items = $startItems })

$bucket = @{}
foreach ($page in $catalog) {
    if ($startOrder -contains $page.Source) { continue }
    if ($page.Source -like 'docs/*') {
        $segment = ($page.Source.Substring(5) -split '/')[0]
        if ($segment -like '*.md') { $segment = 'other' }
    } else {
        $segment = 'repo'
    }
    if (-not $bucket.ContainsKey($segment)) { $bucket[$segment] = New-Object System.Collections.Generic.List[object] }
    $bucket[$segment].Add($page)
}

function Sort-Pages($Items) {
    return @($Items | Sort-Object @{ Expression = { if ($_.Source -match '/README\.md$' -or $_.Source -eq 'README.md') { '0' } else { '1' } } }, Title, Source)
}

$folderOrder = @(
    'editor', 'hub', 'architecture', 'api', 'rendering', 'materials', 'terrain', 'assets',
    'animation', 'physics', 'ai', 'audio', 'networking', 'scripting', 'plugins',
    'build', 'cli', 'profiling', 'testing', 'platform', 'adr', 'rfc', 'research', 'legal', 'repo', 'other'
)
foreach ($key in $folderOrder) {
    if (-not $bucket.ContainsKey($key)) { continue }
    $name = $groupNames[$key]
    if (-not $name) {
        if ($key -eq 'repo') { $name = 'Repository' } else { $name = $key }
    }
    $groups.Add([pscustomobject]@{ Name = $name; Key = $key; Items = (Sort-Pages $bucket[$key]) })
}

function Build-Sidebar($Current) {
    $sb = New-Object System.Text.StringBuilder
    foreach ($group in $groups) {
        $contains = $false
        foreach ($item in $group.Items) {
            if ($item.Output -eq $Current) { $contains = $true }
        }
        $open = ''
        if ($group.Key -eq 'start' -or $contains) { $open = ' open' }
        [void]$sb.Append("<details$open>")
        [void]$sb.Append('<summary>' + (Encode-Html $group.Name) + '</summary><ul>')
        foreach ($item in $group.Items) {
            $href = Get-RelHref $Current $item.Output
            $cls = ''
            if ($item.Output -eq $Current) { $cls = ' class="active"' }
            [void]$sb.Append('<li><a' + $cls + ' href="' + (Encode-Html $href) + '">' + (Encode-Html $item.Title) + '</a></li>')
        }
        [void]$sb.Append('</ul></details>')
    }
    return $sb.ToString()
}

function Build-Pager($Page) {
    $siblings = $null
    foreach ($group in $groups) {
        $match = $false
        foreach ($item in $group.Items) {
            if ($item.Source -eq $Page.Source) { $match = $true }
        }
        if ($match) {
            $copy = New-Object System.Collections.Generic.List[object]
            foreach ($item in $group.Items) { $copy.Add($item) }
            $siblings = $copy.ToArray()
            break
        }
    }
    if (-not $siblings) { return '' }
    $index = 0
    for ($n = 0; $n -lt $siblings.Count; $n++) {
        if ($siblings[$n].Source -eq $Page.Source) { $index = $n }
    }
    $html = '<nav class="pager">'
    if ($index -gt 0) {
        $prev = $siblings[$index - 1]
        $html += '<a href="' + (Encode-Html (Get-RelHref $Page.Output $prev.Output)) + '">Previous: ' + (Encode-Html $prev.Title) + '</a>'
    } else {
        $html += '<span></span>'
    }
    if ($index -lt ($siblings.Count - 1)) {
        $next = $siblings[$index + 1]
        $html += '<a href="' + (Encode-Html (Get-RelHref $Page.Output $next.Output)) + '">Next: ' + (Encode-Html $next.Title) + '</a>'
    }
    $html += '</nav>'
    return $html
}

$search = New-Object System.Collections.Generic.List[string]
foreach ($page in $catalog) {
    $ctx = @{ Source = $page.Source; Output = $page.Output }
    $converted = Convert-Markdown $page.Raw $ctx
    $tocHtml = '<nav class="toc"><p>On this page</p><ul>'
    foreach ($entry in $converted.Toc) {
        $tocHtml += '<li class="toc-h' + $entry.Level + '"><a href="#' + (Encode-Html $entry.Id) + '">' + (Encode-Html $entry.Text) + '</a></li>'
    }
    $tocHtml += '</ul></nav>'
    $indexHref = Get-RelHref $page.Output 'assets/search-index.json'
    $homeHref = Get-RelHref $page.Output 'manual/README.html'
    $sidebar = Build-Sidebar $page.Output
    $pager = Build-Pager $page
    $title = Encode-Html $page.Title
    $html = '<!DOCTYPE html>' + "`n" +
        '<html lang="en"><head><meta charset="utf-8">' +
        '<meta name="viewport" content="width=device-width, initial-scale=1">' +
        '<title>' + $title + ' - JARVIG</title>' +
        '<link rel="stylesheet" href="' + (Encode-Html (Get-RelHref $page.Output 'assets/site.css')) + '">' +
        '</head><body>' +
        '<header class="top"><a class="brand" href="' + (Encode-Html $homeHref) + '">JARVIG</a><span>Manual</span>' +
        '<form class="search" role="search"><input id="q" type="search" placeholder="Search the manual" data-index="' + (Encode-Html $indexHref) + '" autocomplete="off"><div id="results" hidden></div></form></header>' +
        '<div class="layout"><nav class="sidebar">' + $sidebar + '</nav>' +
        '<main><article>' + $converted.Html + '</article>' + $pager + '</main>' +
        $tocHtml + '</div>' +
        '<script src="' + (Encode-Html (Get-RelHref $page.Output 'assets/search.js')) + '"></script>' +
        '</body></html>'
    $dest = Join-Path $site ($page.Output -replace '/', '\')
    $destDir = Split-Path -Parent $dest
    if (-not (Test-Path -LiteralPath $destDir)) {
        New-Item -ItemType Directory -Path $destDir -Force | Out-Null
    }
    [System.IO.File]::WriteAllText($dest, $html, $utf8)
    $plain = [regex]::Replace($page.Raw, '(?s)```.*?```', ' ')
    $plain = [regex]::Replace($plain, '\s+', ' ').Trim()
    if ($plain.Length -gt 280) { $plain = $plain.Substring(0, 280) }
    $search.Add('{"title":"' + (Encode-Json $page.Title) + '","href":"' + (Encode-Json $page.Output) + '","text":"' + (Encode-Json $plain) + '"}')
}

$index = '<!DOCTYPE html><html lang="en"><head><meta charset="utf-8">' +
    '<meta http-equiv="refresh" content="0; url=manual/README.html">' +
    '<title>JARVIG manual</title><link rel="stylesheet" href="assets/site.css"></head>' +
    '<body><main><p><a href="manual/README.html">Open the JARVIG manual</a></p></main></body></html>'
[System.IO.File]::WriteAllText((Join-Path $site 'index.html'), $index, $utf8)

$notice = @"
This folder is generated by scripts/build-docs.ps1.
It is a local reader for the Markdown under docs/ and a few repository entry pages.
Do not copy it into the public tree.
Do not robocopy it onto the release candidate.
Session logs, the ticket board, benchmark dumps, and the private surface-rule pages are not in this folder.
The creator manual does not teach that rule.
"@
[System.IO.File]::WriteAllText((Join-Path $site 'LOCAL_ONLY.txt'), $notice.Trim() + "`n", $utf8)
[System.IO.File]::WriteAllText((Join-Path $site 'assets\search-index.json'), '[' + ($search -join ',') + ']', $utf8)

$css = @'
:root {
  color-scheme: dark;
  --bg: #1a1c1f;
  --panel: #23262b;
  --panel-2: #2c3036;
  --text: #e6e8eb;
  --muted: #a7adb6;
  --accent: #4c8dff;
  --line: #3a3f46;
  --code: #14161a;
  --table: #202328;
}
* { box-sizing: border-box; }
html, body { margin: 0; padding: 0; background: var(--bg); color: var(--text); font: 16px/1.55 "Segoe UI", sans-serif; }
a { color: var(--accent); text-decoration: none; }
a:hover { text-decoration: underline; }
.top {
  display: flex; align-items: center; gap: 16px;
  height: 56px; padding: 0 20px; background: #14161a;
  border-bottom: 1px solid var(--line); position: sticky; top: 0; z-index: 2;
}
.brand { color: var(--text); font-weight: 650; letter-spacing: 0.04em; }
.top span { color: var(--muted); }
.search { margin-left: auto; position: relative; }
.search input {
  width: 280px; background: var(--panel); color: var(--text);
  border: 1px solid var(--line); border-radius: 6px; padding: 8px 10px;
}
#results {
  position: absolute; right: 0; top: 40px; width: 420px; max-height: 420px; overflow: auto;
  background: var(--panel); border: 1px solid var(--line); border-radius: 8px; padding: 8px;
}
#results a { display: block; padding: 8px; border-radius: 6px; color: var(--text); }
#results a:hover { background: var(--panel-2); text-decoration: none; }
#results span { display: block; color: var(--muted); font-size: 13px; }
.layout { display: grid; grid-template-columns: 280px minmax(0, 1fr) 220px; gap: 8px; }
.sidebar, .toc {
  position: sticky; top: 56px; height: calc(100vh - 56px); overflow: auto;
  padding: 12px 12px 48px; background: var(--panel);
}
.sidebar details { margin: 0 0 8px; }
.sidebar summary { cursor: pointer; color: var(--muted); font-size: 13px; letter-spacing: 0.04em; text-transform: uppercase; }
.sidebar ul { list-style: none; margin: 6px 0 0; padding: 0; }
.sidebar a { display: block; padding: 4px 8px; border-radius: 4px; color: var(--text); font-size: 14px; }
.sidebar a.active, .sidebar a:hover { background: var(--panel-2); text-decoration: none; }
.sidebar a.active { box-shadow: inset 2px 0 0 var(--accent); }
main { padding: 28px 36px 80px; max-width: 920px; }
article h1 { font-size: 34px; line-height: 1.2; margin: 0 0 16px; }
article h2 { font-size: 24px; margin: 32px 0 10px; padding-top: 8px; }
article h3 { font-size: 18px; margin: 24px 0 8px; }
article p, article li { color: var(--text); }
article ul, article ol { padding-left: 22px; }
pre {
  background: var(--code); border: 1px solid var(--line); border-radius: 8px;
  padding: 14px 16px; overflow: auto; font: 13px/1.45 Consolas, monospace;
}
code { font-family: Consolas, monospace; font-size: 0.92em; }
p code, li code, td code { background: var(--code); padding: 1px 5px; border-radius: 4px; }
table { border-collapse: collapse; width: 100%; margin: 12px 0 20px; background: var(--table); }
th, td { border: 1px solid var(--line); text-align: left; padding: 8px 10px; vertical-align: top; }
th { background: #2a2e34; }
blockquote { margin: 12px 0; padding: 4px 14px; border-left: 3px solid var(--accent); color: var(--muted); }
.repo-only { border-bottom: 1px dotted var(--muted); cursor: help; }
.toc p { margin: 4px 8px 8px; color: var(--muted); font-size: 13px; text-transform: uppercase; letter-spacing: 0.04em; }
.toc ul { list-style: none; margin: 0; padding: 0; }
.toc a { display: block; color: var(--muted); font-size: 13px; padding: 3px 8px; }
.toc .toc-h3 a { padding-left: 18px; }
.pager { display: flex; justify-content: space-between; gap: 16px; margin-top: 36px; padding-top: 16px; border-top: 1px solid var(--line); }
img { max-width: 100%; height: auto; }
@media (max-width: 960px) {
  .layout { display: block; }
  .sidebar, .toc { position: relative; top: 0; height: auto; }
  .toc { display: none; }
  .search input, #results { width: min(280px, 70vw); }
  main { padding: 20px 16px 48px; }
}
'@
$js = @'
(function () {
  var input = document.getElementById("q");
  var box = document.getElementById("results");
  if (!input || !box) return;
  var data = [];
  function prefix() {
    var path = String(location.pathname || "").replace(/\\/g, "/").toLowerCase();
    var marker = "/docs/site/";
    var at = path.indexOf(marker);
    if (at < 0) return "";
    var rest = path.substring(at + marker.length).split("/").filter(Boolean);
    rest.pop();
    return rest.map(function () { return "../"; }).join("");
  }
  function esc(value) {
    return String(value).replace(/[&<>"]/g, function (ch) {
      return ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[ch];
    });
  }
  var root = prefix();
  fetch(input.getAttribute("data-index")).then(function (response) {
    return response.json();
  }).then(function (json) {
    data = json;
  }).catch(function () {
    data = [];
  });
  input.addEventListener("input", function () {
    var query = input.value.trim().toLowerCase();
    if (query.length < 2) {
      box.hidden = true;
      box.innerHTML = "";
      return;
    }
    var hits = [];
    for (var i = 0; i < data.length && hits.length < 12; i++) {
      var blob = (data[i].title + " " + data[i].text).toLowerCase();
      if (blob.indexOf(query) !== -1) hits.push(data[i]);
    }
    box.hidden = false;
    if (!hits.length) {
      box.innerHTML = "<p>No matches</p>";
      return;
    }
    box.innerHTML = hits.map(function (hit) {
      return '<a href="' + esc(root + hit.href) + '"><strong>' + esc(hit.title) + "</strong><span>" + esc(hit.text) + "</span></a>";
    }).join("");
  });
})();
'@
[System.IO.File]::WriteAllText((Join-Path $site 'assets\site.css'), $css, $utf8)
[System.IO.File]::WriteAllText((Join-Path $site 'assets\search.js'), $js, $utf8)

# The public generator checks that the manual pages exist and that excluded
# folders were not written. It does not list private research terms.
$siteNeedles = @()
$manualNeedles = @()
$manualOutputs = @(
    'manual/README.html', 'getting-started.html', 'manual/editor.html', 'manual/modeling.html',
    'manual/content.html', 'manual/worlds.html', 'manual/lighting.html', 'manual/play.html',
    'manual/programming.html', 'manual/coordinates.html', 'manual/limits.html', 'manual/authority.html'
)
$failures = New-Object System.Collections.Generic.List[string]
$htmlFiles = Get-ChildItem -LiteralPath $site -Recurse -Filter *.html -File
foreach ($file in $htmlFiles) {
    $text = [System.IO.File]::ReadAllText($file.FullName)
    foreach ($needle in $siteNeedles) {
        if ($text.Contains($needle)) {
            $failures.Add("site needle '$needle' in $($file.FullName)")
        }
    }
}
foreach ($rel in $manualOutputs) {
    $path = Join-Path $site ($rel -replace '/', '\')
    if (-not (Test-Path -LiteralPath $path)) {
        $failures.Add("missing manual page $rel")
        continue
    }
    $text = [System.IO.File]::ReadAllText($path)
    if ($text.Length -lt 500) { $failures.Add("manual page too small $rel") }
    foreach ($needle in $manualNeedles) {
        if ($text.Contains($needle)) { $failures.Add("manual needle '$needle' in $rel") }
    }
}
$bannedOutputs = @(
    'rendering\microgeometry.html',
    'research\aperiodic-detail.html',
    'rfc\RFC-0002-procedural-microgeometry.html',
    'status',
    'benchmarks'
)
foreach ($rel in $bannedOutputs) {
    $path = Join-Path $site $rel
    if (Test-Path -LiteralPath $path) { $failures.Add("excluded path was emitted: $rel") }
}
if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Host $_ }
    throw "DOCS_SITE_FAIL $($failures.Count)"
}
Write-Host "DOCS_SITE_PASS pages=$($catalog.Count) html=$($htmlFiles.Count)"
