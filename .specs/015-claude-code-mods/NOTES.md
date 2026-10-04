# Notes — 015

Ruling: lib.ts helpers take `io: Io` (contract.ts) instead of `$`, because `claude plugin validate` refuses `$` passed across an import ("$ is followed only into a function declared in this same file, never across an import"). Each hook file builds its own `makeIo($)`; a closure over `$` passed as an argument validates (probed 2026-10-04). T003 reworked to match before tick.
