"""One restart-scoped policy for optional terminal motion."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Final


@dataclass(frozen=True)
class ScrollMotion:
    """Arguments shared by Textual's scroll methods."""

    animate: bool
    duration: float | None = None


@dataclass(frozen=True)
class MotionPolicy:
    """Route decorative frames and scroll easing through one setting.

    Correctness timers and elapsed-time text do not enter this policy. They
    continue updating in reduced mode because they communicate state rather
    than decorate it.
    """

    reduced: bool = False

    def progress_frame(self, frames: tuple[str, ...], index: int) -> str:
        """Return one decorative frame, or the documented static replacement."""
        if self.reduced:
            return "[..]"
        if not frames:
            return ""
        return frames[index % len(frames)]

    def progress_text(self, label: str, *, ordinary: str) -> str:
        """Use the current copy, or the documented static reduced form."""
        return f"[..] {label}" if self.reduced else ordinary

    def scroll(self, *, animate: bool, duration: float | None = None) -> ScrollMotion:
        """Keep an existing scroll choice, except reduced mode is immediate."""
        if self.reduced:
            return ScrollMotion(animate=False, duration=0.0)
        return ScrollMotion(animate=animate, duration=duration)


STANDARD_MOTION: Final[MotionPolicy] = MotionPolicy()
REDUCED_MOTION: Final[MotionPolicy] = MotionPolicy(reduced=True)

__all__ = ["MotionPolicy", "REDUCED_MOTION", "STANDARD_MOTION", "ScrollMotion"]
