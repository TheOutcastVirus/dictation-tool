import threading

_tk_available = False
try:
    import tkinter as tk
    _tk_available = True
except Exception:
    pass


class Indicator:
    def __init__(self):
        self._root = None
        self._window = None
        self._enabled = False
        self._thread = None

    def start(self):
        """Start tkinter in a background thread. Silently disabled if no display."""
        if not _tk_available:
            return
        ready = threading.Event()
        self._thread = threading.Thread(target=self._run, args=(ready,), daemon=True)
        self._thread.start()
        ready.wait(timeout=2)

    def _run(self, ready: threading.Event):
        try:
            self._root = tk.Tk()
            self._root.withdraw()
            self._window = None
            self._enabled = True
            ready.set()
            self._root.mainloop()
        except Exception as e:
            print(f"[indicator] disabled: {e}", flush=True)
            ready.set()

    def show(self):
        if self._enabled and self._root:
            self._root.after(0, self._show_window)

    def hide(self):
        if self._enabled and self._root:
            self._root.after(0, self._hide_window)

    def _show_window(self):
        if self._window is not None:
            return
        win = tk.Toplevel(self._root)
        win.overrideredirect(True)
        win.attributes("-topmost", True)
        win.configure(bg="#222222")

        canvas = tk.Canvas(win, width=16, height=16, bg="#222222", highlightthickness=0)
        canvas.create_oval(2, 2, 14, 14, fill="#ff3333", outline="")
        canvas.pack(side=tk.LEFT, padx=(8, 4), pady=8)

        label = tk.Label(win, text="Recording...", fg="white", bg="#222222",
                         font=("Sans", 11, "bold"))
        label.pack(side=tk.LEFT, padx=(0, 12), pady=8)

        win.update_idletasks()
        sw = win.winfo_screenwidth()
        w = win.winfo_width()
        x = (sw - w) // 2
        win.geometry(f"+{x}+20")

        self._window = win

    def _hide_window(self):
        if self._window is not None:
            self._window.destroy()
            self._window = None
