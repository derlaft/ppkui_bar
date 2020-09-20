# pepekroll-bar

![screenshot](https://user-images.githubusercontent.com/852606/93707503-7e58b980-fb2f-11ea-8ed3-7de3eda9f8c8.png)

pepekroll-bar is a native wayland client for providing a bottom touch bar.

## Usage

```bash
pepekroll-bar -- \
  -b '⮹' 'echo B1' \
  -b '🡄' 'echo B2' \
  -b '🡆' 'echo B3' \
  -b '⌨' 'echo B4' \
  --height 64
```

#### Missing features

Several features have not yet been implemented:

* Configuration and theming
* Testing on a real pinephone

Overall it is quite rough around the edges at this point, although it is usable.

## License

Licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
