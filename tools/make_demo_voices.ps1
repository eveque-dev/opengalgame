param([string]$Project = (Join-Path $PSScriptRoot '../examples/tidal-stage/project.json'))
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Speech
$og5ProjectPath = (Resolve-Path -LiteralPath $Project).Path
$og5Data = Get-Content -LiteralPath $og5ProjectPath -Raw -Encoding UTF8 | ConvertFrom-Json
$og5Root = Split-Path -Parent $og5ProjectPath
$og5Synth = New-Object System.Speech.Synthesis.SpeechSynthesizer
try {
    $og5Installed = @($og5Synth.GetInstalledVoices() | Where-Object Enabled | ForEach-Object { $_.VoiceInfo.Name })
    if (-not ($og5Installed -contains 'Microsoft Huihui Desktop')) { throw 'The demo requires the installed Microsoft Huihui Desktop Chinese voice.' }
    $og5Synth.Rate = -1
    foreach ($og5Node in $og5Data.nodes) {
        $og5Voice = if ($og5Node.character -in @('lin','cen') -and $og5Installed -contains 'Microsoft Kangkang') { 'Microsoft Kangkang' } elseif ($og5Node.character -eq 'shen' -and $og5Installed -contains 'Microsoft Yaoyao') { 'Microsoft Yaoyao' } else { 'Microsoft Huihui Desktop' }
        $og5Synth.SelectVoice($og5Voice)
        $og5Output = Join-Path $og5Root ('assets/' + $og5Node.id + '.wav')
        $og5Synth.SetOutputToWaveFile($og5Output)
        $og5Synth.Speak([string]$og5Node.text)
        $og5Synth.SetOutputToNull()
        Write-Output ('Rendered voice: ' + $og5Node.id)
    }
} finally { $og5Synth.Dispose() }
