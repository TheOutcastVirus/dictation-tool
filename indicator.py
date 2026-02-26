import tkinter as tk
import threading


class Indicator:
    def __init__(self):
        self._root = None
        self._thread = None
        self._ready = threading.Event()

    def start(self):
        """Start the tkinter mainloop in the main thread (call once at startup)."""
        self._root = tk.Tk()
        self._root.withdraw()
        self._window = None
        self._ready.set()

    def show(self):
        if self._root is None:
            return
        self._root.after(0, self._show_window)

    def hide(self):
        if self._root is None:
            return
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

        # Position top-center
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

    def mainloop(self):
        """Block forever running tkinter event loop."""
        self._root.mainloop()

    def quit(self):
        if self._root:
            self._root.after(0, self._root.quit)
