extern crate lodepng;
extern crate lodepng_unittest;

#[test]
fn test1() {
    unsafe {
        assert_eq!(0, lodepng_unittest::lode_unittest_main1());
    }
}
#[test]
fn test2() {
    unsafe {
        assert_eq!(0, lodepng_unittest::lode_unittest_main2());
    }
}
#[test]
fn test3() {
    unsafe {
        assert_eq!(0, lodepng_unittest::lode_unittest_main3());
    }
}
#[test]
fn test4() {
    unsafe {
        assert_eq!(0, lodepng_unittest::lode_unittest_main4());
    }
}
#[test]
fn test5() {
    unsafe {
        assert_eq!(0, lodepng_unittest::lode_unittest_main5());
    }
}
#[test]
fn test6() {
    unsafe {
        assert_eq!(0, lodepng_unittest::lode_unittest_main6());
    }
}
#[test]
fn test7() {
    unsafe {
        assert_eq!(0, lodepng_unittest::lode_unittest_main7());
    }
}
#[test]
fn test8() {
    unsafe {
        assert_eq!(0, lodepng_unittest::lode_unittest_main8());
    }
}
