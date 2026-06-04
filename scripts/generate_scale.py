#!/usr/bin/env python3
"""Generate JSON scale configs for hyprgram frequency grid overlay."""

import argparse
import json
import sys
from dataclasses import dataclass
from typing import Optional


@dataclass
class StyleDef:
    color: list[int]
    width: int

    def to_dict(self):
        return {"color": self.color, "width": self.width}


@dataclass
class LineStyles:
    root: Optional[StyleDef] = None
    octave: Optional[StyleDef] = None
    default: Optional[StyleDef] = None

    def to_dict(self):
        d = {}
        if self.root:
            d["root"] = self.root.to_dict()
        if self.octave:
            d["octave"] = self.octave.to_dict()
        if self.default:
            d["default"] = self.default.to_dict()
        return d


# Scale interval definitions (semitones from root)
SCALES = {
    "chromatic": list(range(12)),
    "major": [0, 2, 4, 5, 7, 9, 11],
    "minor": [0, 2, 3, 5, 7, 8, 10],
    "pentatonic_major": [0, 2, 4, 7, 9],
    "pentatonic_minor": [0, 3, 5, 7, 10],
    "blues": [0, 3, 5, 6, 7, 10],
    "dorian": [0, 2, 3, 5, 7, 9, 10],
    "mixolydian": [0, 2, 4, 5, 7, 9, 10],
    "harmonic_minor": [0, 2, 3, 5, 7, 8, 11],
    "melodic_minor": [0, 2, 3, 5, 7, 9, 11],
}

# Note name mappings
NOTE_NAMES_SHARP = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"]
NOTE_NAMES_FLAT = ["C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B"]


def note_to_semitones(note: str) -> int:
    """Convert note name (e.g., 'C4', 'A#3', 'Bb2') to semitones from C0."""
    note = note.strip().upper()
    accidental = 0
    
    if "#" in note or "SHARP" in note:
        accidental = 1
        note = note.replace("#", "").replace("SHARP", "")
    elif "B" in note[1:] or "FLAT" in note:  # B after first char (so we don't match 'B' note)
        accidental = -1
        note = note.replace("b", "").replace("FLAT", "").replace("B", "", 1)
    
    base_note = note[0]
    octave = int("".join(c for c in note if c.isdigit()) or 0)
    
    base_semitones = {"C": 0, "D": 2, "E": 4, "F": 5, "G": 7, "A": 9, "B": 11}
    
    return base_semitones.get(base_note, 0) + accidental + (octave * 12)


def semitones_to_note(semitones: int, use_sharp: bool = True) -> str:
    """Convert semitones from C0 to note name."""
    note_idx = semitones % 12
    octave = semitones // 12
    names = NOTE_NAMES_SHARP if use_sharp else NOTE_NAMES_FLAT
    return f"{names[note_idx]}{octave}"


def freq_for_note(semitones: int, a4_ref: float = 440.0) -> float:
    """Calculate frequency for a given semitone (A4 = 440Hz default)."""
    a4_semitones = note_to_semitones("A4")
    semitone_diff = semitones - a4_semitones
    return a4_ref * (2 ** (semitone_diff / 12))


def generate_scale(
    root_note: str,
    scale_type: str,
    octaves: tuple[int, int],
    a4_ref: float = 440.0,
    use_sharp: bool = True,
) -> dict:
    """Generate a scale configuration."""
    
    intervals = SCALES.get(scale_type, SCALES["chromatic"])
    root_semitones = note_to_semitones(root_note)
    root_freq = freq_for_note(root_semitones, a4_ref)
    
    # Default styles
    styles = LineStyles(
        root=StyleDef(color=[255, 100, 100], width=2),
        octave=StyleDef(color=[200, 200, 200], width=1),
        default=StyleDef(color=[150, 150, 150], width=1),
    )
    
    lines = []
    
    for oct in range(octaves[0], octaves[1] + 1):
        for interval in intervals:
            semitones = (oct * 12) + interval
            freq = freq_for_note(semitones, a4_ref)
            note_name = semitones_to_note(semitones, use_sharp)
            is_root = interval == 0
            
            style = styles.root if is_root else styles.default
            
            lines.append({
                "freq": round(freq, 2),
                "label": note_name,
                "color": style.color,
                "width": style.width,
                "style": "root" if is_root else None,
            })
    
    return {
        "name": f"{scale_type.capitalize()} {root_note} ({octaves[0]}-{octaves[1]})",
        "source": {
            "generated": {
                "root_note": root_note,
                "root_freq": round(root_freq, 2),
                "scale_type": scale_type,
                "octaves": {"start": octaves[0], "end": octaves[1]},
            }
        },
        "styles": styles.to_dict(),
    }


def generate_custom(
    name: str,
    frequencies: list[float],
    labels: Optional[list[str]] = None,
    color: list[int] = [200, 200, 200],
    width: int = 1,
) -> dict:
    """Generate a custom frequency list configuration."""
    
    lines = []
    for i, freq in enumerate(frequencies):
        label = labels[i] if labels and i < len(labels) else None
        line = {
            "freq": freq,
            "color": color,
            "width": width,
        }
        if label:
            line["label"] = label
        lines.append(line)
    
    return {
        "name": name,
        "source": {"custom": {"lines": lines}},
    }


def main():
    parser = argparse.ArgumentParser(
        description="Generate scale configs for hyprgram frequency grid overlay"
    )
    parser.add_argument(
        "output",
        nargs="?",
        help="Output JSON file (default: stdout)",
    )
    parser.add_argument(
        "--root", "-r",
        default="C",
        help="Root note (e.g., C, A, F#) (default: C)"
    )
    parser.add_argument(
        "--scale", "-s",
        default="chromatic",
        choices=list(SCALES.keys()) + ["custom"],
        help="Scale type (default: chromatic)"
    )
    parser.add_argument(
        "--octaves",
        default="1-7",
        help="Octave range like '1-7' (default: 1-7)"
    )
    parser.add_argument(
        "--a4",
        type=float,
        default=440.0,
        help="A4 reference frequency (default: 440.0)"
    )
    parser.add_argument(
        "--flat", "-b",
        action="store_true",
        help="Use flat note names (default: sharp)"
    )
    parser.add_argument(
        "--name", "-n",
        help="Custom name for the scale"
    )
    
    # Custom frequency list options
    parser.add_argument(
        "--freqs",
        help="Comma-separated frequencies for custom scale"
    )
    parser.add_argument(
        "--labels",
        help="Comma-separated labels for custom scale"
    )
    
    args = parser.parse_args()
    
    # Parse octave range
    try:
        oct_start, oct_end = map(int, args.octaves.split("-"))
    except ValueError:
        print(f"Invalid octave range: {args.octaves}", file=sys.stderr)
        sys.exit(1)
    
    # Generate scale
    if args.scale == "custom":
        if not args.freqs:
            print("--freqs required for custom scale", file=sys.stderr)
            sys.exit(1)
        freqs = [float(f.strip()) for f in args.freqs.split(",")]
        labels = None
        if args.labels:
            labels = [lbl.strip() for lbl in args.labels.split(",")]
        config = generate_custom(
            args.name or "Custom frequencies",
            freqs,
            labels,
        )
    else:
        config = generate_scale(
            args.root,
            args.scale,
            (oct_start, oct_end),
            args.a4,
            not args.flat,
        )
        if args.name:
            config["name"] = args.name
    
    # Output
    json_str = json.dumps(config, indent=2)
    
    if args.output:
        with open(args.output, "w") as f:
            f.write(json_str)
        print(f"Scale config written to: {args.output}")
        print(f"  {config.get('_comment', '') or len(config.get('source', {}).get('custom', {}).get('lines', []))} lines")
    else:
        print(json_str)


if __name__ == "__main__":
    main()
