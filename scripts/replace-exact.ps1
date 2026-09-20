[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string]$File,

    [Parameter(Mandatory)]
    [string]$OldFile,

    [Parameter(Mandatory)]
    [string]$NewFile,

    [switch]$Apply
)

$ErrorActionPreference = 'Stop'

function Read-Utf8Text([string]$Path) {
    $bytes = [IO.File]::ReadAllBytes($Path)
    $hasBom = $bytes.Length -ge 3 -and $bytes[0] -eq 0xEF -and $bytes[1] -eq 0xBB -and $bytes[2] -eq 0xBF
    $offset = if ($hasBom) { 3 } else { 0 }
    $encoding = [Text.UTF8Encoding]::new($false, $true)
    $text = $encoding.GetString($bytes, $offset, $bytes.Length - $offset)
    [pscustomobject]@{
        Text = $text
        HasBom = $hasBom
    }
}

function Convert-LineEndings([string]$Text, [string]$Eol) {
    $normalized = $Text.Replace("`r`n", "`n").Replace("`r", "`n")
    $normalized.Replace("`n", $Eol)
}

try {
    $source = Read-Utf8Text $File
    $old = (Read-Utf8Text $OldFile).Text
    $new = (Read-Utf8Text $NewFile).Text

    if ([string]::IsNullOrEmpty($old)) {
        throw 'Old fragment must not be empty.'
    }
    if ($source.Text -eq $old) {
        throw 'Whole-file replacement is not allowed.'
    }

    $crlfCount = [regex]::Matches($source.Text, "`r`n").Count
    $lfCount = [regex]::Matches($source.Text, "(?<!`r)`n").Count
    if ($crlfCount -gt 0 -and $lfCount -gt 0) {
        throw 'Target file has mixed line endings.'
    }
    $eol = if ($crlfCount -gt 0) { "`r`n" } else { "`n" }

    $old = Convert-LineEndings $old $eol
    $new = Convert-LineEndings $new $eol
    $matches = [regex]::Matches($source.Text, [regex]::Escape($old)).Count
    if ($matches -ne 1) {
        throw "Expected exactly one matching fragment, found $matches."
    }

    $updated = $source.Text.Replace($old, $new)
    if ($updated -eq $source.Text) {
        throw 'Replacement would not change the file.'
    }

    if ($Apply) {
        $payload = [Text.UTF8Encoding]::new($false).GetBytes($updated)
        if ($source.HasBom) {
            $payload = [byte[]](0xEF, 0xBB, 0xBF) + $payload
        }
        [IO.File]::WriteAllBytes($File, $payload)
        Write-Output "Applied one UTF-8 fragment to $File."
    } else {
        Write-Output "Dry run passed for $File; use -Apply to write one UTF-8 fragment."
    }
} catch {
    Write-Error "replace-exact: $($_.Exception.Message)"
    exit 1
}
