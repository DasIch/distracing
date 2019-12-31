#[cfg(feature = "lightstep")]
extern crate bytes;
#[cfg(feature = "lightstep")]
extern crate prost;
#[cfg(feature = "lightstep")]
extern crate prost_types;

#[cfg(feature = "lightstep")]
mod lightstep;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
