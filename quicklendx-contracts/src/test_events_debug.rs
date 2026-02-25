#[cfg(test)]
mod test {
    use soroban_sdk::{Env, testutils::Events, Symbol, TryFromVal, Val, Address, Vec, xdr};

    #[test]
    fn test_symbol_conversion() {
        let env = Env::default();
        let sym = Symbol::new(&env, "test");
        let val = sym.to_val();
        
        // Correct associated function call if trait is in scope
        // Actually, TryFromVal::try_from_val(&env, &val) is safer if it's on the trait
        let _sym = Symbol::try_from_val(&env, &val).unwrap();
    }

    #[test]
    fn test_vec_conversion() {
        let env = Env::default();
        let events = env.events().all();
        // Since PartialEq is implemented, maybe we can't easily iterate.
        // But we can check equality.
        let expected: Vec<(Address, Vec<Val>, Val)> = Vec::new(&env);
        assert_eq!(events, expected);
    }
}
