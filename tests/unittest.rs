extern crate lodepng;
#[cfg(feature = "c_statics")]
extern crate lodepng_unittest;

#[cfg(feature = "c_statics")]
#[test]
fn test1() {
    unsafe {
        assert_eq!(0, lodepng_unittest::lode_unittest_main1());
    }
}
#[cfg(feature = "c_statics")]
#[test]
fn test2() {
    unsafe {
        assert_eq!(0, lodepng_unittest::lode_unittest_main2());
    }
}
#[cfg(feature = "c_statics")]
#[test]
fn test3() {
    unsafe {
        assert_eq!(0, lodepng_unittest::lode_unittest_main3());
    }
}
#[cfg(feature = "c_statics")]
#[test]
fn test4() {
    unsafe {
        assert_eq!(0, lodepng_unittest::lode_unittest_main4());
    }
}
#[cfg(feature = "c_statics")]
#[test]
fn test5() {
    unsafe {
        assert_eq!(0, lodepng_unittest::lode_unittest_main5());
    }
}
#[cfg(feature = "c_statics")]
#[test]
fn test6() {
    unsafe {
        assert_eq!(0, lodepng_unittest::lode_unittest_main6());
    }
}
#[cfg(feature = "c_statics")]
#[test]
fn test7() {
    unsafe {
        assert_eq!(0, lodepng_unittest::lode_unittest_main7());
    }
}
#[cfg(feature = "c_statics")]
#[test]
fn test8() {
    unsafe {
        assert_eq!(0, lodepng_unittest::lode_unittest_main8());
    }
}
