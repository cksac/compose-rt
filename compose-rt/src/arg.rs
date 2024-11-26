pub trait ArgType {
    type Type;
}

pub trait Arg<T>: 'static {
    fn arg(&self) -> T;
}

impl<T, U> Arg<T> for U
where
    U: ArgType + private::ToArg<U::Type, T> + 'static,
{
    fn arg(&self) -> T {
        self.to_arg()
    }
}

mod private {
    use super::ArgType;

    pub mod marker {
        pub struct Value;

        pub struct Fn;
    }

    pub trait ToArg<M, T> {
        fn to_arg(&self) -> T;
    }

    impl<U, T> ToArg<marker::Value, T> for U
    where
        T: From<U>,
        U: ArgType + Clone,
    {
        fn to_arg(&self) -> T {
            T::from(self.clone())
        }
    }

    impl<F, U, T> ToArg<marker::Fn, T> for F
    where
        F: Fn() -> U + ArgType,
        T: From<U>,
    {
        fn to_arg(&self) -> T {
            T::from(self())
        }
    }

    // implementations
    impl<F, T> ArgType for F
    where
        F: Fn() -> T,
    {
        type Type = marker::Fn;
    }

    impl<'a> ArgType for &'a str {
        type Type = marker::Value;
    }

    impl ArgType for String {
        type Type = marker::Value;
    }
}
