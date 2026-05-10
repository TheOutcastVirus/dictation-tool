"""Type text directly via uinput.

Per-character keystroke injection on GNOME/Mutter Wayland is racy at high
event rates: the compositor's xkbcommon modifier state updates asynchronously
relative to uinput events, which corrupts capitalization and shifted
punctuation when events arrive faster than ~1 ms apart. Real USB keyboards
at 1000 Hz polling already enforce that minimum gap; we go well above it.

The delays below are deliberately generous (typing speed ≈ 80 cps), chosen
to sit well above the timing thresholds that triggered the corruption seen
during faster-injection attempts. A typical dictated sentence (~80 chars)
takes roughly 1 second to type — slower than instant but reliable.
"""

import time
from evdev import UInput, ecodes as e

# US QWERTY: char -> (evdev keycode, needs shift)
_CHAR_MAP: dict[str, tuple[int, bool]] = {
    'a': (e.KEY_A, False), 'b': (e.KEY_B, False), 'c': (e.KEY_C, False),
    'd': (e.KEY_D, False), 'e': (e.KEY_E, False), 'f': (e.KEY_F, False),
    'g': (e.KEY_G, False), 'h': (e.KEY_H, False), 'i': (e.KEY_I, False),
    'j': (e.KEY_J, False), 'k': (e.KEY_K, False), 'l': (e.KEY_L, False),
    'm': (e.KEY_M, False), 'n': (e.KEY_N, False), 'o': (e.KEY_O, False),
    'p': (e.KEY_P, False), 'q': (e.KEY_Q, False), 'r': (e.KEY_R, False),
    's': (e.KEY_S, False), 't': (e.KEY_T, False), 'u': (e.KEY_U, False),
    'v': (e.KEY_V, False), 'w': (e.KEY_W, False), 'x': (e.KEY_X, False),
    'y': (e.KEY_Y, False), 'z': (e.KEY_Z, False),
    'A': (e.KEY_A, True),  'B': (e.KEY_B, True),  'C': (e.KEY_C, True),
    'D': (e.KEY_D, True),  'E': (e.KEY_E, True),  'F': (e.KEY_F, True),
    'G': (e.KEY_G, True),  'H': (e.KEY_H, True),  'I': (e.KEY_I, True),
    'J': (e.KEY_J, True),  'K': (e.KEY_K, True),  'L': (e.KEY_L, True),
    'M': (e.KEY_M, True),  'N': (e.KEY_N, True),  'O': (e.KEY_O, True),
    'P': (e.KEY_P, True),  'Q': (e.KEY_Q, True),  'R': (e.KEY_R, True),
    'S': (e.KEY_S, True),  'T': (e.KEY_T, True),  'U': (e.KEY_U, True),
    'V': (e.KEY_V, True),  'W': (e.KEY_W, True),  'X': (e.KEY_X, True),
    'Y': (e.KEY_Y, True),  'Z': (e.KEY_Z, True),
    '1': (e.KEY_1, False), '2': (e.KEY_2, False), '3': (e.KEY_3, False),
    '4': (e.KEY_4, False), '5': (e.KEY_5, False), '6': (e.KEY_6, False),
    '7': (e.KEY_7, False), '8': (e.KEY_8, False), '9': (e.KEY_9, False),
    '0': (e.KEY_0, False),
    ' ':  (e.KEY_SPACE,      False), '\n': (e.KEY_ENTER,      False),
    '\t': (e.KEY_TAB,        False),
    '`':  (e.KEY_GRAVE,      False), '~':  (e.KEY_GRAVE,      True),
    '-':  (e.KEY_MINUS,      False), '_':  (e.KEY_MINUS,      True),
    '=':  (e.KEY_EQUAL,      False), '+':  (e.KEY_EQUAL,      True),
    '[':  (e.KEY_LEFTBRACE,  False), '{':  (e.KEY_LEFTBRACE,  True),
    ']':  (e.KEY_RIGHTBRACE, False), '}':  (e.KEY_RIGHTBRACE, True),
    '\\': (e.KEY_BACKSLASH,  False), '|':  (e.KEY_BACKSLASH,  True),
    ';':  (e.KEY_SEMICOLON,  False), ':':  (e.KEY_SEMICOLON,  True),
    "'":  (e.KEY_APOSTROPHE, False), '"':  (e.KEY_APOSTROPHE, True),
    ',':  (e.KEY_COMMA,      False), '<':  (e.KEY_COMMA,      True),
    '.':  (e.KEY_DOT,        False), '>':  (e.KEY_DOT,        True),
    '/':  (e.KEY_SLASH,      False), '?':  (e.KEY_SLASH,      True),
    '!':  (e.KEY_1, True),  '@':  (e.KEY_2, True),  '#':  (e.KEY_3, True),
    '$':  (e.KEY_4, True),  '%':  (e.KEY_5, True),  '^':  (e.KEY_6, True),
    '&':  (e.KEY_7, True),  '*':  (e.KEY_8, True),  '(':  (e.KEY_9, True),
    ')':  (e.KEY_0, True),
}

# Normalize common Unicode chars Whisper may produce to ASCII equivalents.
_NORMALIZE = str.maketrans({
    '‘': "'", '’': "'",   # curly single quotes
    '“': '"', '”': '"',   # curly double quotes
    '–': '-', '—': '-',   # en/em dash
    '…': '...',                # ellipsis
})

# Aggressive timing — pushed near the theoretical floor (USB HID polling at
# 1000 Hz = 1 ms minimum between events on real hardware). If shifted-char
# corruption reappears (e.g. "Hello" -> "hEllo", "?" -> "/"), bump
# _MODIFIER_SETTLE_S first; that one races the xkbcommon state update and
# was the parameter that demonstrably broke at 2 ms in earlier testing.
_MODIFIER_SETTLE_S = 0.003
_KEY_HOLD_S = 0.001
_INTER_CHAR_S = 0.001


class Typer:
    def __init__(self):
        keys = sorted(set(kc for kc, _ in _CHAR_MAP.values()) | {e.KEY_LEFTSHIFT})
        self._ui = UInput({e.EV_KEY: keys}, name='dictation-uinput')
        time.sleep(0.5)  # let kernel register the new device

    def type(self, text: str) -> None:
        text = text.translate(_NORMALIZE)
        ui = self._ui
        shift = False
        for ch in text:
            entry = _CHAR_MAP.get(ch)
            if entry is None:
                continue
            kc, need_shift = entry

            if need_shift != shift:
                ui.write(e.EV_KEY, e.KEY_LEFTSHIFT, 1 if need_shift else 0)
                ui.syn()
                shift = need_shift
                time.sleep(_MODIFIER_SETTLE_S)

            ui.write(e.EV_KEY, kc, 1)
            ui.syn()
            time.sleep(_KEY_HOLD_S)
            ui.write(e.EV_KEY, kc, 0)
            ui.syn()
            time.sleep(_INTER_CHAR_S)

        if shift:
            ui.write(e.EV_KEY, e.KEY_LEFTSHIFT, 0)
            ui.syn()

    def close(self) -> None:
        self._ui.close()
