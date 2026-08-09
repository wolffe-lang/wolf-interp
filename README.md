# wolf-interp

The wolf reference interpreter: an independent implementation of the wolf
language specification, and the compiler's differential-testing oracle.

Independence is the point: this repo shares **no** frontend or semantics
code with the compiler ([wolf-lang](https://github.com/tenseleyFlow/wolf-lang)).
The only shared artifacts are the spec and corpus it pins, and the
differential protocol (spec/06) both implementations speak.

Dual-licensed MIT or Apache-2.0.
