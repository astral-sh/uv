# PubGrub version solving algorithm

![license](https://img.shields.io/crates/l/pubgrub.svg)
[![crates.io](https://img.shields.io/crates/v/pubgrub.svg?logo=rust)][crates]
[![docs.rs](https://img.shields.io/badge/docs.rs-pubgrub-yellow)][docs]
[![guide](https://img.shields.io/badge/guide-pubgrub-pink?logo=read-the-docs)][guide]

Version solving consists in efficiently finding a set of packages and versions
that satisfy all the constraints of a given project dependencies.
In addition, when that is not possible,
PubGrub tries to provide a very human-readable and clear
explanation as to why that failed.
The [introductory blog post about PubGrub][medium-pubgrub] presents
one such example of failure explanation:

```txt
Because dropdown >=2.0.0 depends on icons >=2.0.0 and
  root depends on icons <2.0.0, dropdown >=2.0.0 is forbidden.

And because menu >=1.1.0 depends on dropdown >=2.0.0,
  menu >=1.1.0 is forbidden.

And because menu <1.1.0 depends on dropdown >=1.0.0 <2.0.0
  which depends on intl <4.0.0, every version of menu
  requires intl <4.0.0.

So, because root depends on both menu >=1.0.0 and intl >=5.0.0,
  version solving failed.
```

This pubgrub crate provides a Rust implementation of PubGrub.
It is generic and works for any type of dependency system
as long as packages (P) and versions (V) implement
the provided `Package` and `Version` traits.


## Using the pubgrub crate

A [guide][guide] with both high-level explanations and
in-depth algorithm details is available online.
The [API documentation is available on docs.rs][docs].
A version of the [API docs for the unreleased functionality][docs-dev] from `dev` branch is also
accessible for convenience.


## Contributing

Discussion and development happens here on GitHub and on our
[Zulip stream](https://rust-lang.zulipchat.com/#narrow/stream/260232-t-cargo.2FPubGrub).
Please join in!

Remember to always be considerate of others,
who may have different native languages, cultures and experiences.
We want everyone to feel welcomed,
let us know with a private message on Zulip if you don't feel that way.


## PubGrub

PubGrub is a version solving algorithm,
written in 2018 by Natalie Weizenbaum
for the Dart package manager.
It is supposed to be very fast and to explain errors
more clearly than the alternatives.
An introductory blog post was
[published on Medium][medium-pubgrub] by its author.

The detailed explanation of the algorithm is
[provided on GitHub][github-pubgrub],
and complemented by the ["Internals" section of our guide][guide-internals].
The foundation of the algorithm is based on ASP (Answer Set Programming),
and a book called
"[Answer Set Solving in Practice][potassco-book]"
by Martin Gebser, Roland Kaminski, Benjamin Kaufmann and Torsten Schaub.

[crates]: https://crates.io/crates/pubgrub
[guide]: https://pubgrub-rs-guide.netlify.app/
[guide-internals]: https://pubgrub-rs-guide.netlify.app/internals/intro.html
[docs]: https://docs.rs/pubgrub
[docs-dev]: https://pubgrub-rs.github.io/pubgrub/pubgrub/
[medium-pubgrub]: https://medium.com/@nex3/pubgrub-2fb6470504f
[github-pubgrub]: https://github.com/dart-lang/pub/blob/master/doc/solver.md
[potassco-book]: https://potassco.org/book/
