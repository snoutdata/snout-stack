# Third-party notices

Code or SQL ported from another project keeps that project's copyright and licence notice,
reproduced here in full, in the same commit that ports it. A dependency pulled in through
Cargo is not listed here: its licence travels with the crate, and `cargo deny check licenses`
(`deny.toml`) decides whether it may be used at all.

## PostgreSQL

`lepis/src/hash.rs` ports Postgres's extended hash functions (`hash_bytes_extended` and
`hash_bytes_uint32_extended` from `src/common/hashfn.c`, which are Bob Jenkins's public-domain
lookup3 as Postgres adapted it, and the per-type wrappers from `src/backend/access/hash/hashfunc.c`
and `src/backend/utils/adt/varchar.c`), plus `hash_numeric_extended` and the digit handling of
`numeric_in`/`numeric_recv` from `src/backend/utils/adt/numeric.c`, so the router computes
exactly the shard a node computes in SQL (L5).

```
PostgreSQL Database Management System
(also known as Postgres, formerly known as Postgres95)

Portions Copyright (c) 1996-2025, The PostgreSQL Global Development Group

Portions Copyright (c) 1994, The Regents of the University of California

Permission to use, copy, modify, and distribute this software and its
documentation for any purpose, without fee, and without a written agreement
is hereby granted, provided that the above copyright notice and this
paragraph and the following two paragraphs appear in all copies.

IN NO EVENT SHALL THE UNIVERSITY OF CALIFORNIA BE LIABLE TO ANY PARTY FOR
DIRECT, INDIRECT, SPECIAL, INCIDENTAL, OR CONSEQUENTIAL DAMAGES, INCLUDING
LOST PROFITS, ARISING OUT OF THE USE OF THIS SOFTWARE AND ITS
DOCUMENTATION, EVEN IF THE UNIVERSITY OF CALIFORNIA HAS BEEN ADVISED OF THE
POSSIBILITY OF SUCH DAMAGE.

THE UNIVERSITY OF CALIFORNIA SPECIFICALLY DISCLAIMS ANY WARRANTIES,
INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY
AND FITNESS FOR A PARTICULAR PURPOSE.  THE SOFTWARE PROVIDED HEREUNDER IS
ON AN "AS IS" BASIS, AND THE UNIVERSITY OF CALIFORNIA HAS NO OBLIGATIONS TO
PROVIDE MAINTENANCE, SUPPORT, UPDATES, ENHANCEMENTS, OR MODIFICATIONS.
```
