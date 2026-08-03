"""The Textual presentation layer — the only package allowed to import a
terminal framework (ADR-0002).

Everything here consumes projection view models
(:class:`talaria.domain.projection.Snapshot` and friends) and holds presentation
state exclusively. No widget reads a gateway, decodes a frame, or decides what a
sub-agent's status means; if a widget needs a derived value, the derivation
belongs in the projection where it can be tested without a screen.
"""
