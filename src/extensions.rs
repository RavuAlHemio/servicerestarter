use std::fmt::Debug;


#[macro_export]
macro_rules! log_panic {
    ($arg0:expr $(, $argn:expr)*) => {
        {
            ::log::error!($arg0 $(, $argn)*);
            ::log::logger().flush();
            panic!($arg0 $(, $argn)*);
        }
    };
}


pub trait ExpectExtension<T> {
    fn expect_log(self, text: &str) -> T;
}
impl<T> ExpectExtension<T> for Option<T> {
    fn expect_log(self, text: &str) -> T {
        match self {
            Some(v) => v,
            None => log_panic!("{}", text),
        }
    }
}
impl<V, E: Debug> ExpectExtension<V> for Result<V, E> {
    fn expect_log(self, text: &str) -> V {
        match self {
            Ok(v) => v,
            Err(e) => log_panic!("{}: {:?}", text, e),
        }
    }
}
