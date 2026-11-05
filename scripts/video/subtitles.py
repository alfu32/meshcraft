"""Subtitle timing and formats for tutorial videos. Pure functions, no I/O or app."""

import hashlib

CHARS_PER_SECOND = 14.0
MIN_CUE_SECONDS = 2.2
LINE_WIDTH = 58
MAX_LINES = 2


# How a line should sound where its written form would be read out badly.
SPOKEN = (
    ("Push/Pull", "push pull"),
    ("30 x11", "thirty, times eleven"),
    ("$NUMBER", "dollar number"),
    ("$HOTSPOT", "dollar hotspot"),
    (" cm", " centimetres"),
    ("Esc", "escape"),
)


def spoken(text):
    for written, said in SPOKEN:
        text = text.replace(written, said)
    return text


def voice_key(text, speaker="ryan"):
    """Clips are cached by what is said, so re-timing a video never regenerates speech."""
    return hashlib.sha256(f"{speaker}:{spoken(text)}".encode()).hexdigest()[:16]


def reading_seconds(text):
    """Time a viewer needs for a cue; also the budget a later voiceover must fit."""
    return max(MIN_CUE_SECONDS, len(text) / CHARS_PER_SECOND) + 0.4


def wrap(text):
    """One line if it fits, otherwise two lines of similar length."""
    if len(text) <= LINE_WIDTH:
        return [text]
    words = text.split()
    splits = [(" ".join(words[:n]), " ".join(words[n:])) for n in range(1, len(words))]
    fitting = [pair for pair in splits if max(map(len, pair)) <= LINE_WIDTH]
    if not fitting:
        raise ValueError(f"Cue needs more than {MAX_LINES} lines: {text}")
    return list(min(fitting, key=lambda pair: abs(len(pair[0]) - len(pair[1]))))


def _clock(seconds, separator):
    milliseconds = round(seconds * 1000)
    hours, milliseconds = divmod(milliseconds, 3_600_000)
    minutes, milliseconds = divmod(milliseconds, 60_000)
    whole, milliseconds = divmod(milliseconds, 1000)
    return f"{hours:02}:{minutes:02}:{whole:02}{separator}{milliseconds:03}"


def _ass_clock(seconds):
    centiseconds = round(seconds * 100)
    hours, centiseconds = divmod(centiseconds, 360_000)
    minutes, centiseconds = divmod(centiseconds, 6000)
    whole, centiseconds = divmod(centiseconds, 100)
    return f"{hours}:{minutes:02}:{whole:02}.{centiseconds:02}"


def srt(cues):
    blocks = []
    for number, cue in enumerate(cues, 1):
        times = f"{_clock(cue['start'], ',')} --> {_clock(cue['end'], ',')}"
        blocks.append(f"{number}\n{times}\n" + "\n".join(wrap(cue["text"])) + "\n")
    return "\n".join(blocks)


def vtt(cues):
    blocks = ["WEBVTT\n"]
    for cue in cues:
        times = f"{_clock(cue['start'], '.')} --> {_clock(cue['end'], '.')}"
        blocks.append(f"{times}\n" + "\n".join(wrap(cue["text"])) + "\n")
    return "\n".join(blocks)


def _escape(text):
    return text.replace("\\", "\\\\").replace("{", "\\{").replace("}", "\\}")


# libass aligns a drawing by its box from the origin, so keep coordinates positive.
CIRCLE = "m 24 0 b 37 0 48 11 48 24 b 48 37 37 48 24 48 b 11 48 0 37 0 24 b 0 11 11 0 24 0"


def overlay(clicks, titles, viewport, keys=(), snaps=(), size=(1920, 1080)):
    """Burn-in script for click rings, keycaps and title cards. Subtitles are never burned in."""
    width, height = size
    head = f"""[Script Info]
ScriptType: v4.00+
PlayResX: {width}
PlayResY: {height}
WrapStyle: 2
ScaledBorderAndShadow: yes

[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding
Style: Key,Liberation Sans,34,&H00FFFFFF,&H00FFFFFF,&H0020A0FF,&H0020A0FF,1,0,0,0,100,100,0,0,3,9,0,9,0,0,0,1
Style: Snap,Liberation Sans,22,&H00FFFFFF,&H00FFFFFF,&H28402820,&H28402820,0,0,0,0,100,100,0,0,3,5,0,7,0,0,0,1
Style: Mark,Liberation Sans,20,&HFF000000,&HFF000000,&H0020A0FF,&H00000000,0,0,0,0,100,100,0,0,1,3,0,5,0,0,0,1
Style: Shade,Liberation Sans,20,&H28181410,&H28181410,&HFF000000,&HFF000000,0,0,0,0,100,100,0,0,1,0,0,7,0,0,0,1
Style: Title,Liberation Sans,76,&H00FFFFFF,&H00FFFFFF,&H00000000,&H00000000,1,0,0,0,100,100,0,0,1,0,0,5,120,120,0,1
Style: Subtitle,Liberation Sans,38,&H00D8D0C8,&H00D8D0C8,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,0,0,5,120,120,0,1

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
"""
    lines = []
    for card in titles:
        start, end = _ass_clock(card["start"]), _ass_clock(card["end"])
        fade = "{\\fad(350,350)}"
        box = f"{{\\pos(0,0)\\p1\\fad(350,350)}}m 0 0 l {width} 0 {width} {height} 0 {height}{{\\p0}}"
        lines.append(f"Dialogue: 5,{start},{end},Shade,,0,0,0,,{box}")
        middle = height // 2
        title = f"{{\\pos({width // 2},{middle - 44})\\fad(350,350)}}{_escape(card['title'])}"
        lines.append(f"Dialogue: 6,{start},{end},Title,,0,0,0,,{title}")
        if card.get("subtitle"):
            text = f"{{\\pos({width // 2},{middle + 44})}}{fade}{_escape(card['subtitle'])}"
            lines.append(f"Dialogue: 6,{start},{end},Subtitle,,0,0,0,,{text}")
    corner = (viewport["x"] + viewport["width"] - 24, viewport["y"] + 24)
    for key in keys:
        text = f"{{\\pos({corner[0]},{corner[1]})\\fad(80,160)}}{_escape(key['text'])}"
        lines.append(
            f"Dialogue: 4,{_ass_clock(key['start'])},{_ass_clock(key['end'])},Key,,0,0,0,,{text}"
        )
    for snap in snaps:
        # Beside the cursor, naming what the app itself reported that it snapped to.
        text = f"{{\\pos({snap['x'] + 26},{snap['y'] + 26})\\fad(80,120)}}{_escape(snap['text'])}"
        lines.append(
            f"Dialogue: 3,{_ass_clock(snap['start'])},{_ass_clock(snap['end'])},Snap,,0,0,0,,{text}"
        )
    for click in clicks:
        grow = "\\fscx40\\fscy40\\t(\\fscx130\\fscy130\\3a&HFF&)"
        ring = f"{{\\pos({click['x']},{click['y']}){grow}\\p1}}{CIRCLE}{{\\p0}}"
        end = click["t"] + 0.45
        lines.append(
            f"Dialogue: 3,{_ass_clock(click['t'])},{_ass_clock(end)},Mark,,0,0,0,,{ring}"
        )
    return head + "\n".join(lines) + "\n"


def audit(cues):
    """Problems a viewer would notice; empty means the track is fit to publish."""
    problems = []
    for index, cue in enumerate(cues):
        shown = cue["end"] - cue["start"]
        if shown + 0.01 < reading_seconds(cue["text"]) - 0.4:
            problems.append(f"cue {index + 1} shown {shown:.2f}s, too fast: {cue['text']}")
        if index and cue["start"] < cues[index - 1]["end"]:
            problems.append(f"cue {index + 1} overlaps the previous cue")
        try:
            wrap(cue["text"])
        except ValueError as error:
            problems.append(str(error))
    return problems
