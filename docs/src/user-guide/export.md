# Exporting Projects

Learn how to export your animations from Motioner.

## Export Options

Motioner supports two primary export formats:

1. **PNG Sequence** — Individual frames as images
2. **MP4 Video** — Encoded video file via FFmpeg

## Exporting Video (MP4)

### Using the UI

1. Click **Export Video** button
2. Choose output location
3. Motioner will:
   - Render each frame as PNG
   - Automatically encode with FFmpeg
   - Save final MP4 file

### Settings

Configure before export:
- **Frame Rate** — Matches project FPS
- **Resolution** — Canvas dimensions
- **Quality** — H.264 encoding settings

### Output Location

Default: `out/` directory in project folder
- Frames: `out/frames/frame_00001.png`
- Video: `out/output.mp4`

## Exporting PNG Sequence

### Frame-by-Frame Export

1. Select **Export Frames** option
2. Choose destination folder
3. Frames saved with sequential naming:
   - `frame_00001.png`
   - `frame_00002.png`
   - `frame_00003.png`
   - ...

### Use Cases

- Post-production in other software
- Custom encoding parameters
- Non-realtime effects processing
- Frame inspection and debugging

## Manual FFmpeg Encoding

### Basic Encoding

```powershell
ffmpeg -framerate 30 -i out/frames/frame_%05d.png `
  -c:v libx264 -pix_fmt yuv420p output.mp4
```

### High Quality

```powershell
ffmpeg -framerate 30 -i out/frames/frame_%05d.png `
  -c:v libx264 -crf 18 -preset slow -pix_fmt yuv420p output.mp4
```

### With Alpha Channel

```powershell
ffmpeg -framerate 30 -i out/frames/frame_%05d.png `
  -c:v png output.mov
```

### GIF Export

```powershell
ffmpeg -framerate 30 -i out/frames/frame_%05d.png `
  -vf "fps=15,scale=800:-1:flags=lanczos" output.gif
```

## FFmpeg Parameters Explained

| Parameter | Purpose | Example |
|-----------|---------|---------|
| `-framerate` | Input frame rate | `30` |
| `-i` | Input pattern | `frame_%05d.png` |
| `-c:v` | Video codec | `libx264` |
| `-crf` | Quality (0-51, lower=better) | `18` |
| `-preset` | Encoding speed/quality | `slow`, `medium`, `fast` |
| `-pix_fmt` | Pixel format | `yuv420p` |

## Best Practices

### Before Exporting

- ✅ Preview entire animation
- ✅ Check frame count
- ✅ Verify resolution
- ✅ Ensure sufficient disk space

### Quality Settings

**Standard Web:**
- Resolution: 1920x1080
- FPS: 30
- CRF: 23

**High Quality:**
- Resolution: 3840x2160 (4K)
- FPS: 60
- CRF: 18

**Quick Preview:**
- Resolution: 1280x720
- FPS: 24
- CRF: 28

### File Size Optimization

Reduce file size:
- Lower resolution
- Reduce frame rate
- Higher CRF value (lower quality)
- Use two-pass encoding

```powershell
# Two-pass encoding (better quality/size ratio)
ffmpeg -framerate 30 -i out/frames/frame_%05d.png `
  -c:v libx264 -b:v 2M -pass 1 -f null NUL

ffmpeg -framerate 30 -i out/frames/frame_%05d.png `
  -c:v libx264 -b:v 2M -pass 2 output.mp4
```

## Troubleshooting

### FFmpeg Not Found

```
Error: FFmpeg executable not found
```

**Solution:**
1. Install FFmpeg
2. Add to system PATH
3. Restart Motioner

### Export Fails Midway

**Common causes:**
- Insufficient disk space
- Frame rendering error
- FFmpeg encoding error

**Solution:**
- Check disk space
- Review console output
- Try manual frame export first

### Quality Issues

**Problem:** Video looks pixelated

**Solution:**
- Lower CRF value (18-20)
- Use slower preset
- Check source resolution

## Next Steps

- [FFmpeg Integration Example](../examples/ffmpeg-integration.md)
- [Troubleshooting Guide](../reference/troubleshooting.md)
