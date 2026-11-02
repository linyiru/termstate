# Interpretations of the spec

Places where the Program Status Protocol (revision 0.4) leaves room, and the
reading termstate implements. Each has an id that test vectors cite in their
`interpretation` field, so the choice is visible, tested, and easy to flip if
the spec settles it differently. Candidates to raise upstream are marked
**ask upstream**.

## INT-1

**A pair named `id` whose value has a byte outside the value set discards the
report.** The Syntax section says a malformed pair is skipped, but skipping
`id` would leave the report addressing the root record, and Records and ids
says: "Falling back to the root record would let a malformed id overwrite
it." The safety rule wins. **ask upstream**

## INT-2

**`id=` (empty value) is an id that does not match the grammar, so the report
is ignored.** A segment needs at least one character; only a report with no
`id` key addresses the root.

## INT-3

**Limits are checked on every occurrence of a repeated key, including
occurrences a later repeat overrides, and every `id` occurrence must match
the grammar. Base64 and control characters are checked only on the `title`
and `msg` that win.** The Limits section says "Check every pair before
touching any stored record"; an overridden text value is never decoded or
shown, so it cannot carry an undecodable or unsafe string. Same as Ghostty.

## INT-4

**A key longer than 16 bytes discards the report even when the key also has
characters outside `[a-z]`.** The length limit is checked first. Either way
a well-formed program never sends such a key.

## INT-5

**`app` longer than 32 bytes discards the report (Limits), while an `app` that
is empty or has characters outside its set is treated as absent (Keys).**
**ask upstream**: the Keys table gives `{1,32}` as part of the character
set rule, which could also read as "too long is absent".

## INT-6

**Decoded `msg` or `title` that is not valid UTF-8 discards the report.** The
spec says the values are "base64 of UTF-8 text" but lists only bad base64 and
control characters as discard reasons.

## INT-7

**`progress` accepts leading zeros (`007` is 7).** "Integer 0 to 100" does not
forbid them. Signs, decimals and empty values are absent.

## INT-8

**"Whitespace" is space (0x20) and tab (0x09).** Other C0 bytes never reach
the parser from the scanner (see INT-10).

## INT-9

**Base64 must be canonical: bits after the last whole byte must be zero**
(`QR==` discards; `QQ==` is `A`). RFC 4648 §3.5 lets decoders reject
non-canonical input, and Ghostty's decoder (Zig `std.base64`) does. Padding,
when present, must be complete and correct.

## INT-10

**Inside the OSC string the scanner ignores C0 controls other than BEL, ESC,
CAN and SUB**, following the DEC/xterm parser that real terminals use. CAN
and SUB abort the sequence. ESC not followed by `\` aborts it and starts a
new escape sequence. 8-bit C1 OSC/ST (0x9D/0x9C) are not recognized.

## INT-11

**A body whose first pair is `?` is a feature-detection message**, with
whitespace around the `?` allowed. From a program it is the query; from a
terminal it is the reply. The parser returns the capabilities the body lists
(spec defaults for anything not listed) and the caller knows which direction
it is reading. Vectors that only make sense as replies carry
`"direction": "reply"`. Ghostty accepts exactly `?` as a query and does not
parse replies (it is terminal-side only).

## INT-12

**Keys a state ignores are dropped at parse time**: `kind` unless `blocked`,
`progress` unless `working` or `blocked`. The record therefore never holds
them, which is the observable meaning of "ignored".

## INT-13

**Eviction happens only when a report would create a new record at the cap.**
Replacing an existing record never evicts. The record being created is
never the one evicted.

## INT-14

**A store's capacity is clamped to 64..=256**: terminals "MAY choose lower
limits" but "MUST support at least 64".

## INT-15

**`state=clear` still goes through full validation**: a clear with an invalid
id, an over-limit pair, or bad base64 in an ignored `msg` is discarded like
any other report.

## INT-16

**An empty `title` or `msg` (`msg=`) is the same as an absent one.** Nothing
distinguishes "no message" from "an empty message" for display, and Ghostty
reads empty text as absent. The builder never sends an empty value.

## INT-17

**A limit binds even on a value that is skipped because of a byte outside the
value set**: `app=` with 33 bytes including a `!` discards the report, while
`app=a!` is skipped. A limit is about how much a terminal must look at, not
about whether the value is usable. Same as Ghostty.
