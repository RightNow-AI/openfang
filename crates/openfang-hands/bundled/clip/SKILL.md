---
name: clip-hand-skill
version: "2.0.0"
description: "Expert knowledge for AI video clipping — yt-dlp downloading, whisper transcription, SRT generation, and ffmpeg processing"
runtime: prompt_only
---

# Video Clipping Expert Knowledge

## Cross-Platform Notes

All tools (ffmpeg, ffprobe, yt-dlp, whisper) use **identical CLI flags** on Windows, macOS, and Linux. The differences are only in shell syntax:

| Feature | macOS / Linux | Windows (cmd.exe) |
|---------|---------------|-------------------|
| Suppress stderr | `2>/dev/null` | `2>NUL` |
| Filter output | `\| grep pattern` | `\| findstr pattern` |
| Delete files | `rm file1 file2` | `del file1 file2` |
| Null output device | `-f null -` | `-f null -` (same) |
| ffmpeg subtitle paths | `subtitles=clip.srt` | `subtitles=clip.srt` (relative OK, absolute needs `C\\:/path`) |

IMPORTANT: ffmpeg filter paths (`-vf "subtitles=..."`) always need forward slashes. On Windows with absolute paths, escape the colon: `subtitles=C\\:/Users/me/clip.srt`

Prefer using `file_write` tool for creating SRT/text files instead of shell echo/heredoc.

---

## yt-dlp Reference

### Download with Format Selection
```
# Best video up to 1080p + best audio, merged
yt-dlp -f "bv[height<=1080]+ba/b[height<=1080]" --restrict-filenames -o "source.%(ext)s" "URL"

# 720p max (smaller, faster)
yt-dlp -f "bv[height<=720]+ba/b[height<=720]" --restrict-filenames -o "source.%(ext)s" "URL"

# Audio only (for transcription-only workflows)
yt-dlp -x --audio-format wav --restrict-filenames -o "audio.%(ext)s" "URL"
```

### Metadata Inspection
```
# Get full metadata as JSON (duration, title, chapters, available subs)
yt-dlp --dump-json "URL"

# Key fields: duration, title, description, chapters, subtitles, automatic_captions
```

### YouTube Auto-Subtitles
```
# Download auto-generated subtitles in json3 format (word-level timing)
yt-dlp --write-auto-subs --sub-lang en --sub-format json3 --skip-download --restrict-filenames -o "source" "URL"

# Download manual subtitles if available
yt-dlp --write-subs --sub-lang en --sub-format srt --skip-download --restrict-filenames -o "source" "URL"

# List available subtitle languages
yt-dlp --list-subs "URL"
```

### Useful Flags
- `--restrict-filenames` — safe ASCII filenames (no spaces/special chars) — important on all platforms
- `--no-playlist` — download single video even if URL is in a playlist
- `-o "template.%(ext)s"` — output template (%(ext)s auto-detects format)
- `--cookies-from-browser chrome` — use browser cookies for age-restricted content
- `--extract-audio` / `-x` — extract audio only
- `--audio-format wav` — convert audio to wav (for whisper)

---

## Whisper Transcription Reference

### Audio Extraction for Whisper
```
# Extract mono 16kHz WAV (whisper's preferred input format)
ffmpeg -i source.mp4 -vn -ar 16000 -ac 1 -y audio.wav
```

### Basic Transcription
```
# Standard transcription with word-level timestamps
whisper audio.wav --model small --output_format json --word_timestamps true --language en

# Faster alternative (same flags, 4x speed)
whisper-ctranslate2 audio.wav --model small --output_format json --word_timestamps true --language en
```

### Model Sizes
| Model | VRAM | Speed | Quality | Use When |
|-------|------|-------|---------|----------|
| tiny | ~1GB | Fastest | Rough | Quick previews, testing pipeline |
| base | ~1GB | Fast | OK | Short clips, clear speech |
| small | ~2GB | Good | Good | **Default — best balance** |
| medium | ~5GB | Slow | Better | Important content, accented speech |
| large-v3 | ~10GB | Slowest | Best | Final production, multiple languages |

Note: On macOS Apple Silicon, consider `mlx-whisper` as a faster native alternative.

### JSON Output Structure
Output contains `text` (full transcript) and `segments[]` with `start`, `end`, `text` fields.
With `--word_timestamps true`: each segment has `words[]` with `word`, `start`, `end`, `probability` (< 0.5 = likely wrong).

---

## YouTube json3 Subtitle Parsing

### Format & Extraction
Structure: `events[].tStartMs` + `events[].segs[].{utf8, tOffsetMs}`

Word timing: `word_start_ms = event.tStartMs + seg.tOffsetMs` → divide by 1000 for seconds.
Skip events without `segs` or with `segs` containing only `"\n"` (formatting/newlines).

---

## SRT Generation from Transcript

### SRT Rules
- Format: `HH:MM:SS,mmm` (comma, not dot) — entry = index + timestamp + text + blank line
- ~8-12 words per line, 2-3 seconds, break at natural pauses, ≤42 chars for mobile
- Adjust timestamps relative to clip start
- Use `file_write` tool to create the SRT file — works identically on all platforms

### Styled Captions with ASS Format
For animated/styled captions, use ASS subtitle format instead of SRT:
```
ffmpeg -i clip.mp4 -vf "subtitles=clip.ass:force_style='FontSize=22,FontName=Arial,Bold=1,PrimaryColour=&H00FFFFFF,OutlineColour=&H00000000,Outline=2,Shadow=1,Alignment=2,MarginV=40'" -c:a copy output.mp4
```

Key ASS style properties:
- `PrimaryColour=&H00FFFFFF` — white text (AABBGGRR format)
- `OutlineColour=&H00000000` — black outline
- `Outline=2` — outline thickness
- `Alignment=2` — bottom center
- `MarginV=40` — margin from bottom edge
- `FontSize=22` — good size for 1080x1920 vertical

---

## FFmpeg Video Processing

### Scene Detection
```
ffmpeg -i input.mp4 -filter:v "select='gt(scene,0.3)',showinfo" -f null - 2>&1
```
- Threshold 0.1 = very sensitive, 0.5 = only major cuts
- Parse `pts_time:` from showinfo output for timestamps
- On macOS/Linux pipe through `grep showinfo`, on Windows pipe through `findstr showinfo`

### Silence Detection
```
ffmpeg -i input.mp4 -af "silencedetect=noise=-30dB:d=1.5" -f null - 2>&1
```
- `d=1.5` = minimum 1.5 seconds of silence
- Look for `silence_start` and `silence_end` in output

### Clip Extraction
```
# Re-encoded (accurate cuts)
ffmpeg -ss 00:01:30 -to 00:02:15 -i input.mp4 -c:v libx264 -c:a aac -preset fast -crf 23 -movflags +faststart -y clip.mp4

# Lossless copy (fast but may have keyframe alignment issues)
ffmpeg -ss 00:01:30 -to 00:02:15 -i input.mp4 -c copy -y clip.mp4
```
- `-ss` before `-i` = fast seek (recommended for extraction)
- `-to` = end timestamp, `-t` = duration

### Vertical Video (9:16 for Shorts/Reels/TikTok)
```
# Center crop (when source is 16:9)
ffmpeg -i input.mp4 -vf "crop=ih*9/16:ih:(iw-ih*9/16)/2:0,scale=1080:1920" -c:a copy output.mp4

# Scale with letterbox padding (preserves full frame)
ffmpeg -i input.mp4 -vf "scale=1080:1920:force_original_aspect_ratio=decrease,pad=1080:1920:(ow-iw)/2:(oh-ih)/2:black" -c:a copy output.mp4
```

### Caption Burn-in
```
# SRT subtitles with styling (use relative path or forward-slash absolute path)
ffmpeg -i input.mp4 -vf "subtitles=subs.srt:force_style='FontSize=22,FontName=Arial,PrimaryColour=&H00FFFFFF,OutlineColour=&H00000000,Outline=2,Alignment=2,MarginV=40'" -c:a copy output.mp4

# Simple text overlay
ffmpeg -i input.mp4 -vf "drawtext=text='Caption':fontsize=48:fontcolor=white:borderw=3:bordercolor=black:x=(w-text_w)/2:y=h-th-40" output.mp4
```
Windows path escaping: `subtitles=C\\:/Users/me/subs.srt` (double-backslash before colon)

### Thumbnail Generation
```
# At specific time (2 seconds in)
ffmpeg -i input.mp4 -ss 2 -frames:v 1 -q:v 2 -y thumb.jpg

# Best keyframe
ffmpeg -i input.mp4 -vf "select='eq(pict_type,I)',scale=1280:720" -frames:v 1 thumb.jpg

# Contact sheet
ffmpeg -i input.mp4 -vf "fps=1/10,scale=320:-1,tile=4x4" contact.jpg
```

### Video Analysis
```
# Full metadata (JSON)
ffprobe -v quiet -print_format json -show_format -show_streams input.mp4

# Duration only
ffprobe -v error -show_entries format=duration -of csv=p=0 input.mp4

# Resolution
ffprobe -v error -select_streams v:0 -show_entries stream=width,height -of csv=p=0 input.mp4
```

## API-Based STT Reference

All APIs accept audio files (max 25MB — split with ffmpeg if larger). Add `timestamp_granularities[]=word` for word timing.

| Provider | Endpoint | Auth Header | Model Param | Env Var |
|----------|----------|-------------|-------------|----------|
| Groq (fastest) | `POST https://api.groq.com/openai/v1/audio/transcriptions` | `Bearer $GROQ_API_KEY` | `whisper-large-v3` | `GROQ_API_KEY` |
| OpenAI | `POST https://api.openai.com/v1/audio/transcriptions` | `Bearer $OPENAI_API_KEY` | `whisper-1` | `OPENAI_API_KEY` |
| Deepgram | `POST https://api.deepgram.com/v1/listen?model=nova-2&smart_format=true&utterances=true&punctuate=true` | `Token $DEEPGRAM_API_KEY` | (in URL) | `DEEPGRAM_API_KEY` |

Groq/OpenAI: multipart form with `-F "file=@audio.wav" -F "response_format=verbose_json"`
Deepgram: binary body with `-H "Content-Type: audio/wav" --data-binary @audio.wav`
Deepgram response nests words under `results.channels[0].alternatives[0].words[]`.

---

## TTS Reference

| Provider | Command / Endpoint | Auth | Notes |
|----------|-------------------|------|-------|
| Edge TTS (free) | `edge-tts --text "TEXT" --voice en-US-AriaNeural --write-media tts.mp3` | None | Install: `pip install edge-tts`. Voices: AriaNeural, GuyNeural, SoniaNeural |
| OpenAI | `POST https://api.openai.com/v1/audio/speech` | `Bearer $OPENAI_API_KEY` | Body: `{"model":"tts-1","input":"TEXT","voice":"alloy"}`. Voices: alloy/echo/fable/onyx/nova/shimmer |
| ElevenLabs | `POST https://api.elevenlabs.io/v1/text-to-speech/VOICE_ID` | `xi-api-key: $ELEVENLABS_API_KEY` | Body: `{"text":"TEXT","model_id":"eleven_monolingual_v1"}`. Default voice ID: `21m00Tcm4TlvDq8ikWAM` (Rachel) |

### Audio Merging (TTS + Original)
```
# Mix TTS over original audio (original at 30% volume, TTS at 100%)
ffmpeg -i clip.mp4 -i tts.mp3 \
  -filter_complex "[0:a]volume=0.3[orig];[1:a]volume=1.0[tts];[orig][tts]amix=inputs=2:duration=first[out]" \
  -map 0:v -map "[out]" -c:v copy -c:a aac -y clip_voiced.mp4

# Replace audio entirely (no original audio)
ffmpeg -i clip.mp4 -i tts.mp3 -map 0:v -map 1:a -c:v copy -c:a aac -shortest -y clip_voiced.mp4
```

---

## Quality & Performance Tips

- Use `-preset ultrafast` for quick previews, `-preset slow` for final output
- Use `-crf 23` for good quality (18=high, 28=low, lower=bigger files)
- Add `-movflags +faststart` for web-friendly MP4
- Use `-threads 0` to auto-detect CPU cores
- Always use `-y` to overwrite without asking

---

## Telegram Bot API Reference

**Send video**: `POST https://api.telegram.org/bot<BOT_TOKEN>/sendVideo`
Form fields: `chat_id` (required), `video=@file.mp4` (max **50MB**), `caption`, `parse_mode=HTML`, `supports_streaming=true`

Size limit fix: `ffmpeg -i input.mp4 -fs 49M -c:v libx264 -crf 28 -preset fast -c:a aac -movflags +faststart -y output.mp4`

| Error | Fix |
|-------|-----|
| 400 chat not found | Bot must be added to channel/group |
| 401 unauthorized | Regenerate token via @BotFather |
| 413 too large | Re-encode under 50MB |
| 429 rate limited | Wait `retry_after` seconds |

---

## WhatsApp Business Cloud API Reference

Two-step flow (upload then send). Auth: `Bearer <ACCESS_TOKEN>`. Max video: **16MB**.

**Step 1 — Upload**: `POST https://graph.facebook.com/v21.0/<PHONE_NUMBER_ID>/media`
Form: `file=@video.mp4`, `type=video/mp4`, `messaging_product=whatsapp` → returns `{"id": "MEDIA_ID"}`

**Step 2 — Send**: `POST https://graph.facebook.com/v21.0/<PHONE_NUMBER_ID>/messages`
JSON body: `{"messaging_product":"whatsapp","to":"PHONE","type":"video","video":{"id":"MEDIA_ID","caption":"text"}}`

Size limit fix: `ffmpeg -i input.mp4 -fs 15M -c:v libx264 -crf 30 -preset fast -c:a aac -movflags +faststart -y output.mp4`

24-hour window: recipient must have messaged you within 24h, otherwise use a pre-approved template.

| Error | Fix |
|-------|-----|
| 100 invalid param | Check phone_number_id, recipient format (no +, no spaces) |
| 190 expired token | Regenerate in Meta Business Settings (temp tokens expire 24h) |
| 131030 not in allowed list | Add recipient in Meta Developer Portal (test mode) |
| 131047 template required | Recipient hasn't messaged within 24h |
| 131053 upload failed | Re-encode as MP4 under 16MB |
