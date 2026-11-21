# Generates examples/design_defects/<rule>/{bad,good}.motion (prompt 13b C2).
# Each pair is the defect and the fixed version the checker is tested against.
$root = Join-Path $PSScriptRoot "..\examples\design_defects"
New-Item -ItemType Directory -Force -Path $root | Out-Null

$comp = 'comp(name="Fixture", width=1000, height=600, fps=60, duration=4.0, background="#101418")'

$fixtures = [ordered]@{
  blank_frame = @(
    $comp,
    ($comp + "`nbg = rect(1000, 600, fill=`"#1B2430`")`nbg.pos = (500, 300)`n")
  )
  off_screen = @(
    ($comp + "`nt = text(`"Off screen`", `"Inter`", 64, `"#FFFFFF`", `"center`")`nt.pos = (-400, -300)`n"),
    ($comp + "`nt = text(`"On screen`", `"Inter`", 64, `"#FFFFFF`", `"center`")`nt.pos = (500, 300)`n")
  )
  safe_area = @(
    ($comp + "`nt = text(`"Too close`", `"Inter`", 64, `"#FFFFFF`", `"left`")`nt.pos = (4, 40)`n"),
    ($comp + "`nt = text(`"Safe`", `"Inter`", 64, `"#FFFFFF`", `"left`")`nt.pos = (100, 120)`n")
  )
  text_overlap = @(
    ($comp + "`na = text(`"Overlapping text`", `"Inter`", 56, `"#FFFFFF`", `"center`")`na.pos = (500, 300)`nb = text(`"second line`", `"Inter`", 56, `"#FFFFFF`", `"center`")`nb.pos = (500, 310)`n"),
    ($comp + "`na = text(`"First line`", `"Inter`", 56, `"#FFFFFF`", `"center`")`na.pos = (500, 240)`nb = text(`"second line`", `"Inter`", 56, `"#FFFFFF`", `"center`")`nb.pos = (500, 360)`n")
  )
  text_on_busy = @(
    ($comp + "`ng = linear_gradient((0, 0), (1000, 600), [(0, `"#000000`"), (0.5, `"#FFFFFF`"), (1, `"#000000`")])`nbg = rect(1000, 600, fill=g)`nbg.pos = (500, 300)`nt = text(`"On a gradient`", `"Inter`", 56, `"#FFFFFF`", `"center`")`nt.pos = (500, 300)`n"),
    ($comp + "`nplate = rect(700, 140, radius=12, fill=`"#1B2430`")`nplate.pos = (500, 300)`nt = text(`"On a surface`", `"Inter`", 56, `"#FFFFFF`", `"center`")`nt.pos = (500, 300)`n")
  )
  contrast = @(
    ('comp(name="Fixture", width=1000, height=600, fps=60, duration=4.0, background="#FFFFFF")' + "`nt = text(`"Invisible`", `"Inter`", 56, `"#FFFFFF`", `"center`")`nt.pos = (500, 300)`n"),
    ('comp(name="Fixture", width=1000, height=600, fps=60, duration=4.0, background="#FFFFFF")' + "`nt = text(`"Readable`", `"Inter`", 56, `"#101010`", `"center`")`nt.pos = (500, 300)`n")
  )
  reading_time = @(
    ($comp.Replace('duration=4.0', 'duration=0.6') + "`nt = text(`"one two three four five six`", `"Inter`", 48, `"#FFFFFF`", `"center`")`nt.pos = (500, 300)`n"),
    ($comp + "`nt = text(`"one two three four five six`", `"Inter`", 48, `"#FFFFFF`", `"center`")`nt.pos = (500, 300)`n")
  )
  velocity_jump = @(
    ($comp + "`nr = rect(120, 120, fill=`"#5B8CFF`")`nr.pos = (120, 300)`nr.key(`"pos.x`", 0.0, 120.0)`nr.key(`"pos.x`", 0.25, 240.0, ease=`"linear`")`nr.key(`"pos.x`", 0.5, 880.0, ease=`"linear`")`n"),
    ($comp + "`nr = rect(120, 120, fill=`"#5B8CFF`")`nr.pos = (120, 300)`nr.key(`"pos.x`", 0.0, 120.0)`nr.key(`"pos.x`", 1.2, 880.0, ease=`"quad_out`")`n")
  )
  peak_speed = @(
    ($comp + "`nr = rect(120, 120, fill=`"#5B8CFF`")`nr.pos = (120, 300)`nr.key(`"pos.x`", 0.0, 120.0)`nr.key(`"pos.x`", 0.05, 880.0, ease=`"linear`")`n"),
    ($comp + "`nr = rect(120, 120, fill=`"#5B8CFF`")`nr.pos = (120, 300)`nr.key(`"pos.x`", 0.0, 120.0)`nr.key(`"pos.x`", 1.2, 880.0, ease=`"quad_out`")`n")
  )
  linear_ease = @(
    ($comp + "`nr = rect(200, 200, fill=`"#5B8CFF`")`nr.pos = (200, 300)`nr.key(`"rotation`", 0.0, 0.0)`nr.key(`"rotation`", 1.0, 90.0, ease=`"linear`")`n"),
    ($comp + "`nr = rect(200, 200, fill=`"#5B8CFF`")`nr.pos = (200, 300)`nr.key(`"rotation`", 0.0, 0.0)`nr.key(`"rotation`", 1.0, 90.0, ease=`"quad_in_out`")`n")
  )
  stagger = @(
    ($comp + "`na = text(`"A`", `"Inter`", 48, `"#FFFFFF`", `"center`")`na.pos = (300, 200)`nb = text(`"B`", `"Inter`", 48, `"#FFFFFF`", `"center`")`nb.pos = (500, 200)`nc = text(`"C`", `"Inter`", 48, `"#FFFFFF`", `"center`")`nc.pos = (700, 200)`na.key(`"opacity`", 0.0, 0.0)`na.key(`"opacity`", 0.6, 1.0, ease=`"quad_out`")`nb.key(`"opacity`", 0.0, 0.0)`nb.key(`"opacity`", 0.6, 1.0, ease=`"quad_out`")`nc.key(`"opacity`", 0.0, 0.0)`nc.key(`"opacity`", 0.6, 1.0, ease=`"quad_out`")`n"),
    ($comp + "`na = text(`"A`", `"Inter`", 48, `"#FFFFFF`", `"center`")`na.pos = (300, 200)`nb = text(`"B`", `"Inter`", 48, `"#FFFFFF`", `"center`")`nb.pos = (500, 200)`nc = text(`"C`", `"Inter`", 48, `"#FFFFFF`", `"center`")`nc.pos = (700, 200)`na.key(`"opacity`", 0.0, 0.0)`na.key(`"opacity`", 0.6, 1.0, ease=`"quad_out`")`nb.key(`"opacity`", 0.1, 0.0)`nb.key(`"opacity`", 0.7, 1.0, ease=`"quad_out`")`nc.key(`"opacity`", 0.2, 0.0)`nc.key(`"opacity`", 0.8, 1.0, ease=`"quad_out`")`n")
  )
  dead_time = @(
    ('comp(name="Fixture", width=1000, height=600, fps=60, duration=8.0, background="#101418")' + "`nr = rect(120, 120, fill=`"#5B8CFF`")`nr.pos = (500, 300)`nr.key(`"opacity`", 0.0, 0.0)`nr.key(`"opacity`", 0.6, 1.0, ease=`"quad_out`")`n"),
    ('comp(name="Fixture", width=1000, height=600, fps=60, duration=8.0, background="#101418")' + "`nr = rect(120, 120, fill=`"#5B8CFF`")`nr.pos = (500, 300)`nr.key(`"rotation`", 0.0, 0.0)`nr.key(`"rotation`", 6.8, 30.0, ease=`"quad_in_out`")`nr.key(`"opacity`", 0.0, 0.0)`nr.key(`"opacity`", 0.6, 1.0, ease=`"quad_out`")`n")
  )
  overload = @(
    ($comp + "`nfor i in range(9):`n    r = rect(200, 160, fill=`"#5B8CFF`")`n    r.pos = (120 + i * 100, 300)`n    r.key(`"pos.y`", 0.0, 300.0)`n    r.key(`"pos.y`", 0.4, 200.0, ease=`"linear`")`n    r.key(`"pos.y`", 0.8, 300.0, ease=`"linear`")`n"),
    ($comp + "`nfor i in range(2):`n    r = rect(200, 160, fill=`"#5B8CFF`")`n    r.pos = (350 + i * 300, 300)`n    r.key(`"pos.y`", 0.0, 300.0)`n    r.key(`"pos.y`", 1.0, 200.0, ease=`"quad_out`")`n    r.key(`"pos.y`", 2.0, 300.0, ease=`"quad_in`")`n")
  )
  type_hierarchy = @(
    ($comp + "`na = text(`"Same size one`", `"Inter`", 40, `"#FFFFFF`", `"center`")`na.pos = (500, 200)`nb = text(`"Same size two`", `"Inter`", 40, `"#FFFFFF`", `"center`")`nb.pos = (500, 300)`nc = text(`"Same size three`", `"Inter`", 40, `"#FFFFFF`", `"center`")`nc.pos = (500, 400)`n"),
    ($comp + "`na = text(`"Headline`", `"Inter`", 72, `"#FFFFFF`", `"center`")`na.pos = (500, 240)`nb = text(`"Body copy`", `"Inter`", 40, `"#D0D6E2`", `"center`")`nb.pos = (500, 340)`nc = text(`"caption`", `"Inter`", 26, `"#A9B8D0`", `"center`")`nc.pos = (500, 410)`n")
  )
}

foreach ($rule in $fixtures.Keys) {
  $dir = Join-Path $root $rule
  New-Item -ItemType Directory -Force -Path $dir | Out-Null
  [IO.File]::WriteAllText((Join-Path $dir "bad.motion"), $fixtures[$rule][0])
  [IO.File]::WriteAllText((Join-Path $dir "good.motion"), $fixtures[$rule][1])
  foreach ($file in @("bad.motion", "good.motion")) {
    $path = Join-Path $dir $file
    $text = [IO.File]::ReadAllText($path)
    [IO.File]::WriteAllText($path, $text.Replace("`r`n", "`n"))
  }
}
Write-Output "wrote $($fixtures.Count * 2) fixture programs to $root"
