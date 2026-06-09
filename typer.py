"""Inject text as keyboard input.

Primary backend — Mutter RemoteDesktop keysym injection
--------------------------------------------------------
Per-character keystroke injection via uinput is racy on GNOME/Mutter
Wayland: the compositor's xkbcommon modifier state updates asynchronously
relative to uinput events, which corrupts capitalization and shifted
punctuation ("Hello" -> "hEllo", "?" -> "/") when events arrive faster
than the compositor processes them. Timing margins reduce but cannot
eliminate the race.

Instead we hand Mutter complete keysyms ("capital H") over its private
session-bus API `org.gnome.Mutter.RemoteDesktop` — the same interface
gnome-remote-desktop uses. Mutter resolves each keysym to keycode +
modifiers on its own input thread, so the race cannot happen by
construction. No permission dialog is shown (unlike the portal flavour of
this API, which on xdg-desktop-portal < 1.16 cannot persist authorization
across restarts). Arbitrary Unicode works via the 0x01000000 + codepoint
keysym convention, and all key events are sent as a single burst so the
full text appears at once, like a paste (~6 ms for 80 chars).

Fallback backend — uinput
-------------------------
Used when the Mutter API is unavailable (non-GNOME compositor) or a call
fails. The delays are deliberately generous (typing speed ~ 80 cps),
chosen to sit well above the timing thresholds that triggered the
corruption seen during faster-injection attempts.
"""

import threading
import time

from evdev import UInput, ecodes as e
from jeepney import DBusAddress, MessageFlag, MessageType, new_method_call
from jeepney.io.blocking import open_dbus_connection

# ── Shared ─────────────────────────────────────────────────────────────────────

# Normalize common Unicode chars Whisper may produce to ASCII equivalents.
_NORMALIZE = str.maketrans({
    '‘': "'", '’': "'",   # curly single quotes
    '“': '"', '”': '"',   # curly double quotes
    '–': '-', '—': '-',   # en/em dash
    '…': '...',                # ellipsis
})

# ── Mutter RemoteDesktop backend ───────────────────────────────────────────────

_RD_BUS = 'org.gnome.Mutter.RemoteDesktop'
_RD_PATH = '/org/gnome/Mutter/RemoteDesktop'
_RD_IFACE = 'org.gnome.Mutter.RemoteDesktop'
_RD_SESSION_IFACE = 'org.gnome.Mutter.RemoteDesktop.Session'

# X11 keysyms for control characters (printable ASCII maps 1:1 to keysyms).
_KEYSYM_SPECIAL = {
    '\n': 0xff0d,  # XK_Return
    '\t': 0xff09,  # XK_Tab
}


def _char_keysym(ch: str) -> int:
    if ch in _KEYSYM_SPECIAL:
        return _KEYSYM_SPECIAL[ch]
    cp = ord(ch)
    if 0x20 <= cp <= 0x7e or 0xa0 <= cp <= 0xff:
        return cp  # ASCII + Latin-1 keysyms equal their codepoints
    return 0x01000000 | cp  # Unicode keysym convention


class _MutterKeyboard:
    def __init__(self):
        self._conn = open_dbus_connection(bus='SESSION')
        rd = DBusAddress(_RD_PATH, bus_name=_RD_BUS, interface=_RD_IFACE)
        reply = self._conn.send_and_get_reply(new_method_call(rd, 'CreateSession'))
        if reply.header.message_type == MessageType.error:
            raise RuntimeError(f'CreateSession failed: {reply.body}')
        self._session = DBusAddress(reply.body[0], bus_name=_RD_BUS,
                                    interface=_RD_SESSION_IFACE)
        self._call('Start')

    def _call(self, method: str, signature: str = None, body: tuple = ()):
        msg = new_method_call(self._session, method, signature, body)
        reply = self._conn.send_and_get_reply(msg)
        if reply.header.message_type == MessageType.error:
            raise RuntimeError(f'{method} failed: {reply.body}')

    def type(self, text: str) -> None:
        # Send all key events as one burst with no per-call round trips, so
        # the whole text lands at once (~6 ms for 80 chars vs ~80 ms when
        # waiting for each reply). Ordering is still guaranteed: the events
        # are serialized on the socket and Mutter processes them in order.
        # The final event does round-trip, fencing the burst and surfacing
        # a dead session as an exception.
        events = []
        for ch in text:
            keysym = _char_keysym(ch)
            events.append((keysym, True))
            events.append((keysym, False))
        if not events:
            return
        for keysym, state in events[:-1]:
            msg = new_method_call(self._session, 'NotifyKeyboardKeysym',
                                  'ub', (keysym, state))
            msg.header.flags |= MessageFlag.no_reply_expected
            self._conn.send(msg)
        self._call('NotifyKeyboardKeysym', 'ub', events[-1])

    def close(self) -> None:
        try:
            self._call('Stop')
        except Exception:
            pass
        try:
            self._conn.close()
        except Exception:
            pass


# ── uinput fallback backend ────────────────────────────────────────────────────

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

# Aggressive timing — pushed near the theoretical floor (USB HID polling at
# 1000 Hz = 1 ms minimum between events on real hardware). If shifted-char
# corruption reappears (e.g. "Hello" -> "hEllo", "?" -> "/"), bump
# _MODIFIER_SETTLE_S first; that one races the xkbcommon state update and
# was the parameter that demonstrably broke at 2 ms in earlier testing.
# Pushing _KEY_HOLD_S or _INTER_CHAR_S below 1 ms causes dropped/duplicated
# characters in some apps.
_MODIFIER_SETTLE_S = 0.003
_KEY_HOLD_S = 0.001
_INTER_CHAR_S = 0.001


class _UinputTyper:
    def __init__(self):
        keys = sorted(set(kc for kc, _ in _CHAR_MAP.values()) | {e.KEY_LEFTSHIFT})
        self._ui = UInput({e.EV_KEY: keys}, name='dictation-uinput')
        time.sleep(0.5)  # let kernel register the new device

    def type(self, text: str) -> None:
        ui = self._ui
        shift = False
        dropped: set[str] = set()
        for ch in text:
            entry = _CHAR_MAP.get(ch)
            if entry is None:
                dropped.add(ch)
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

        if dropped:
            print(f"[typer] dropped unmappable characters: {sorted(dropped)!r}",
                  flush=True)

    def close(self) -> None:
        self._ui.close()


# ── Public interface ───────────────────────────────────────────────────────────

class Typer:
    def __init__(self):
        self._lock = threading.Lock()
        self._uinput: _UinputTyper | None = None
        self._mutter: _MutterKeyboard | None = None
        try:
            self._mutter = _MutterKeyboard()
            print("[typer] using Mutter RemoteDesktop keysym injection", flush=True)
        except Exception as exc:
            print(f"[typer] Mutter RemoteDesktop unavailable ({exc}); "
                  f"using uinput typing", flush=True)
            self._uinput = _UinputTyper()

    def type(self, text: str) -> None:
        text = text.translate(_NORMALIZE)
        if not text:
            return
        with self._lock:
            if self._mutter is not None:
                try:
                    self._mutter.type(text)
                    return
                except Exception as exc:
                    # Session died (e.g. gnome-shell restart) — one reconnect
                    # attempt, then permanent fallback to uinput.
                    print(f"[typer] keysym injection failed ({exc}); "
                          f"recreating session", flush=True)
                    self._mutter.close()
                    try:
                        self._mutter = _MutterKeyboard()
                        self._mutter.type(text)
                        return
                    except Exception as exc2:
                        print(f"[typer] reconnect failed ({exc2}); "
                              f"falling back to uinput typing", flush=True)
                        self._mutter = None
            if self._uinput is None:
                self._uinput = _UinputTyper()
            self._uinput.type(text)

    def close(self) -> None:
        with self._lock:
            if self._mutter is not None:
                self._mutter.close()
                self._mutter = None
            if self._uinput is not None:
                self._uinput.close()
                self._uinput = None
