
mod lookup;

use std::sync::Arc;
use async_trait::async_trait;
use solana_program::pubkey::Pubkey;
use workflow_allocator::result::Result;
use workflow_allocator::accounts::AccountDataReference;
use solana_program::instruction::Instruction;
use downcast::{downcast_sync, AnySync};

#[async_trait(?Send)]
pub trait Interface : AnySync {
    fn get_authority_pubkey(&self) -> Result<Pubkey>;

    async fn execute(&self, instr : &Instruction) -> Result<()>;
    async fn lookup(&self, pubkey:&Pubkey) -> Result<Option<Arc<AccountDataReference>>>;
    async fn lookup_local(&self, pubkey:&Pubkey) -> Result<Option<Arc<AccountDataReference>>>;
    async fn lookup_remote(&self, pubkey:&Pubkey) -> Result<Option<Arc<AccountDataReference>>>;
    
    fn purge(&self, pubkey:&Pubkey) -> Result<()>;

}

downcast_sync!(dyn Interface);

mod config;
pub use config::*;

mod loaders;
pub use loaders::*;

mod transaction;
pub use transaction::*;

mod queue;
pub use queue::*;

mod observer;
pub use observer::*;

#[cfg(target_arch = "wasm32")]
mod wasm;
#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
pub use native::*;
