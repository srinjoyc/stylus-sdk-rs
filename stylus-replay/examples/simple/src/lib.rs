
extern crate alloc;

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

use stylus_sdk::{alloy_primitives::U256, prelude::*, contract, call::Call};

sol_storage! {
    #[entrypoint]
    pub struct Contract {
        uint256 number;
    }
}

sol_interface! {
    interface IContract {
        function recurse(uint8 levels) external;
    }
}

#[external]
impl Contract {
    pub fn number(&self) -> Result<U256, Vec<u8>> {
        Ok(self.number.get())
    }

    pub fn set_number(&mut self, new_number: U256) -> Result<(), Vec<u8>> {
        self.number.set(new_number);
        Ok(())
    }

    pub fn increment(&mut self) -> Result<(), Vec<u8>> {
        let number = self.number.get();
        self.set_number(number + U256::from(1))
    }

    pub fn revert() -> Result<(), Vec<u8>> {
        Err(vec![0xaa, 0xff])
    }

    pub fn recurse(&mut self, levels: u8) -> Result<(), Vec<u8>> {
        if levels > 0 {
            let abi = IContract::new(contract::address());
            abi.recurse(Call::new_in(self), levels - 1)?;
            abi.recurse(Call::new_in(self), levels - 1)?;
        }
        Ok(())
    }
}
