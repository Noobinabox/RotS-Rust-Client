# WIP.md

## Active Slice

None. Slice 54 (Persistent Command History) and slice 58 (Configurable Built-In Shortcuts) are complete and reviewed. See `/help history` for optional history persistence and privacy controls. See `/help keybindings` for typed actions, examples, modal precedence, and recovery guidance.

## Next Slice: 59. Session Transcripts and Output Export

- Add opt-in gameplay transcripts and retained-output export, separate from diagnostic logging.
- Define timestamps, plain-text/ANSI options, size limits, and sensitive-input exclusions.
- Keep file writes off UI/network paths; test file failures and shutdown flushing.
