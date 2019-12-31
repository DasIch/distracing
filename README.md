# DisTracing

An OpenTracing library written in Rust.

## Motivation

So far there are a few attempts to create an OpenTracing library however no
library exists that has support for multiple tracers.

The goal of this project is to offer a library that does work with multiple
tracers and is easy to use.


## Development

### Building the Documentation

For development you are going to want to build the documentation with private
items, to also include documentation on generated code for protocol buffers.

```
cargo doc --document-private-items --all-features
```
