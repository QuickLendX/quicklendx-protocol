pub fn set_symbol(env: Env, symbol: Symbol) -> Result<(), Error> {
    if symbol.len() > 9 {
        return Err(Error::InvalidSymbolLength);
    }
    
    // ... existing logic ...
}
