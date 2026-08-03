"""talaria/recorder — the redaction boundary and frame-log v1 writer/reader.

Below the ADR-0002 projection boundary (this package never imports
``textual`` or ``talaria.ui``): it is pure I/O and data transformation, and its
correctness is proved against the TypeScript reference (``src/record/``) by
``tests/recorder/test_equivalence.py``.
"""
