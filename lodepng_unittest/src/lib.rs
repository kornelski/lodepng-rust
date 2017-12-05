extern crate lodepng;

extern "C" {
    pub fn lode_unittest_main1() -> std::os::raw::c_uint;
    pub fn lode_unittest_main2() -> std::os::raw::c_uint;
    pub fn lode_unittest_main3() -> std::os::raw::c_uint;
    pub fn lode_unittest_main4() -> std::os::raw::c_uint;
    pub fn lode_unittest_main5() -> std::os::raw::c_uint;
    pub fn lode_unittest_main6() -> std::os::raw::c_uint;
    pub fn lode_unittest_main7() -> std::os::raw::c_uint;
    pub fn lode_unittest_main8() -> std::os::raw::c_uint;
    pub fn lode_unittest_main9() -> std::os::raw::c_uint;
}

#[test]
fn test1() {
    unsafe {
        assert_eq!(0, lode_unittest_main1());
    }
}
#[test]
fn test2() {
    unsafe {
        assert_eq!(0, lode_unittest_main2());
    }
}
#[test]
fn test3() {
    unsafe {
        assert_eq!(0, lode_unittest_main3());
    }
}
#[test]
fn test4() {
    unsafe {
        assert_eq!(0, lode_unittest_main4());
    }
}
#[test]
fn test5() {
    unsafe {
        assert_eq!(0, lode_unittest_main5());
    }
}
#[test]
fn test6() {
    unsafe {
        assert_eq!(0, lode_unittest_main6());
    }
}
#[test]
fn test7() {
    unsafe {
        assert_eq!(0, lode_unittest_main7());
    }
}
#[test]
fn test8() {
    unsafe {
        assert_eq!(0, lode_unittest_main8());
    }
}
#[test]
fn test9() {
    unsafe {
        assert_eq!(0, lode_unittest_main9());
    }
}
