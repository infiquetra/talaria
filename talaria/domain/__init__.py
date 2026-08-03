"""Talaria's domain core.

Framework-independent by ADR-0002: nothing under this package may import the
presentation framework (Textual) or ``talaria.ui``. The boundary is enforced by
``tests/domain/test_boundary.py``, not by convention alone.
"""
