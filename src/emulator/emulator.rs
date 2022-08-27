use std::rc::Rc;
use std::sync::{Arc, Mutex};
use ahash::AHashSet;
use async_trait::async_trait;
use solana_program::instruction::Instruction;
use solana_program::pubkey::Pubkey;
use solana_program::rent::Rent;
use solana_program::sysvar::slot_history::AccountInfo;
use solana_program::account_info::IntoAccountInfo;
use solana_program::instruction::AccountMeta;
use solana_program::entrypoint::ProcessInstruction;
use workflow_log::*;
use workflow_allocator::context::SimulationHandlerFn;
use workflow_allocator::result::Result;
use workflow_allocator::error::*;
use workflow_allocator::context::Context;
use workflow_allocator::accounts::*;
use workflow_allocator::builder::{
    InstructionBuilder,
};
use workflow_allocator::accounts::AccountData;
use workflow_allocator::store;
use workflow_allocator::utils;


use super::interface::{EmulatorInterface, ExecutionResponse};

#[derive(Clone)]
pub struct LogSink {
    logs : Arc<Mutex<Option<Vec<String>>>>
}

impl LogSink {
    fn new() -> Self {
        Self {
            logs : Arc::new(Mutex::new(None))
        }
    }
    fn init(&self) {
        *self.logs.lock().unwrap() = Some(Vec::new());
    }
    fn take(&self) -> Vec<String> {
        self.logs.lock().unwrap().take().unwrap()
    }
}

impl workflow_log::Sink for LogSink {
    fn write(&self, _level:Level, args : &std::fmt::Arguments<'_>) -> bool {
        if let Some(logs) = self.logs.lock().unwrap().as_mut() {
            logs.push(args.to_string());
        }
        false
    }
}    


pub struct Emulator {
    store : Arc<dyn store::Store>,
    log_sink : Arc<dyn Sink>
    // capture : AtomicBool,
}



impl Emulator {

    pub fn new(
        store : Arc<dyn store::Store>,
    ) -> Self {
        
        let log_sink: Arc<dyn Sink> = Arc::new(LogSink::new());
        workflow_log::pipe(Some(log_sink.clone()));

        Emulator { 
            store,
            log_sink
            // capture: AtomicBool::new(false),
            // logs: Arc::new(Mutex::new(None))

        }
    }

    pub async fn init(&self) -> Result<()> {
        let mut default = AccountData::default();
        default.lamports = utils::u64sol_to_lamports(500_000_000);
        self.store.store(&Arc::new(AccountDataReference::new(default))).await?;
        Ok(())
    }

    pub fn execute_entrypoing_impl(
        &self,
        program_id: &Pubkey,
        // accounts : &Arc<Mutex<Vec<AccountInfo>>>,
        accounts : &Vec<AccountInfo>,
        instruction_data: &[u8],
        entrypoint: ProcessInstruction,

    ) -> Result<()> {
        log_trace!("▷ entrypoint begin");
        // let accounts = accounts.lock().unwrap();
        match entrypoint(program_id, &accounts[..], instruction_data) {
            Ok(_) => {}
            Err(e) => return Err(error!("entrypoint error: {:?}", e)),
        }
        log_trace!("◁ entrypoint end");
        Ok(())
    }

    pub async fn execute_handler(
        self : Arc<Self>,
        builder: InstructionBuilder,
        handler: SimulationHandlerFn,
    ) -> Result<()> {

        // let store = self.store();
        let ec: Instruction = builder.try_into()?;
        let mut account_data = self.program_local_load(&ec.program_id, &ec.accounts).await?;
        let accounts = Arc::new(Mutex::new(Vec::new()));
        for (pubkey, account_data) in account_data.iter_mut() {
            let is_signer = account_data.is_signer;
            let is_writable = account_data.is_writable;
            let mut account_info = (&*pubkey, account_data).into_account_info();

            // pass signer and writer flags from the source account
            account_info.is_signer = is_signer;
            account_info.is_writable = is_writable;

            accounts.lock().unwrap().push(account_info);
        }
        let accounts = accounts.lock().unwrap();
        let ctx: Context = (
            &ec.program_id,
            &accounts[..],
            ec.data.as_slice(),
        )
            .try_into()
            .expect("Unable to create context");
        match handler(&Rc::new(ctx)) {
            //?;//.map_err(|err| format!("(handler) program error: {:?}", err).to_string())?;
            Ok(_) => {}
            Err(err) => {
                log_trace!("{}", err);
                return Err(err);
                // return Err(err.message());
            }
        }
        // self.program_local_store(&accounts).await?;

        Ok(())
    }

    // pub fn get_account_data(&self, pubkey: &Pubkey) -> Result<Option<Arc<RwLock<AccountData>>>> {
    //     Ok(self.store.lookup(pubkey)?)
    //     // let store = self.store();
    //     // let account_data = store.lookup(pubkey)?;
    //     // let account_data = match account_data {
    //     //     Some(account_data) => Some(account_data.clone()),
    //     //     None => None,
    //     // };
    //     // Ok(account_data)
    // }



    /// Load multiple accounts from local store for test program usage
    // pub async fn program_local_load(self : Arc<Self>, program_id : &Pubkey, accounts : &[AccountMeta]) -> Result<Vec<(Pubkey,AccountData)>> {
    pub async fn program_local_load(&self, program_id : &Pubkey, accounts : &[AccountMeta]) -> Result<Vec<(Pubkey,AccountData)>> {

        // let self_ = self.clone();
        let mut keyset = AHashSet::<Pubkey>::new();

        let mut account_data_vec = Vec::new();
        for descriptor in accounts.iter() {
            let pubkey = descriptor.pubkey;

            if keyset.contains(&pubkey) {
                // log_trace!("Duplicate account supplied to local load: {}", pubkey.to_string());
                return Err(
                    error!("[store] Store::program_local_load(): duplicate account supplied to program: {}",pubkey.to_string())
                );
            } else {
                keyset.insert(pubkey.clone());
            }

            let mut account_data = match self.lookup(&pubkey).await? {
                Some(reference) => {
                    let account_data = reference.clone_for_program().await;//account_data.clone_for_prog//read().await.ok_or(error!("account read lock failed"))?.clone_for_program();

                    log_trace!("[store] ...  loading: {}", account_data.info()?);

                    // log_trace!("... loading account: {} data len: {} lamports: {}", 
                    //     account_data.key.to_string(),
                    //     account_data.data.len(),
                    //     account_data.lamports
                    // );
    
                    account_data
                },
                None => {

                    log_trace!("*\n*\n*\n*\n*\nCLONING ACCOUNT {}\n*\n*\n*\n*",pubkey);

                    let account_data = AccountData::new_template_for_program(
                    // let account_data = AccountData::new_allocated_for_program(
                        pubkey.clone(),
                        program_id.clone(),
                        //pubkey.clone(),
                        0
                    );

                    if pubkey == Pubkey::default() {
                        log_trace!("[store] ...   system: {}", account_data.info()?);
                    } else {
                        log_trace!("[store] ... template: {}", account_data.info()?);
                    }

                    // log_trace!("... template account: {} data len: {} lamports: {}", 
                    //     account_data.key.to_string(),
                    //     account_data.data.len(),
                    //     account_data.lamports
                    // );

                    account_data
                }
            };

            account_data.is_signer = descriptor.is_signer;
            account_data.is_writable = descriptor.is_writable;

            account_data_vec.push((pubkey,account_data));
        }

        // let mut account_data = Vec::new();
        // for (pubkey,account_ref_cell) in account_ref_cells.iter() {
        //     account_data.push((pubkey.clone(),account_ref_cell.borrow().clone()));
        // }

        Ok(account_data_vec)
    }


    // pub async fn program_local_store<'t>(&self, accounts : &Arc<Mutex<Vec<AccountInfo<'t>>>>) -> Result<()> {
    pub async fn program_local_store<'t>(&self, accounts : Vec<(Pubkey,AccountData)>) -> Result<()> {
        // pub async fn program_local_store(&self, accounts : Vec<AccountInfo>) -> Result<()> {
        // pub async fn program_local_store<'info>(&self, accounts : Vec<AccountInfo>) -> Result<()> {

        // let accounts = accounts.lock().unwrap();
        for (pubkey, account_data) in accounts.iter() {
            if let Some(existing_account_data) = self.store.lookup(&account_data.key).await? {
                let existing_account_data = existing_account_data.account_data.read().await;//.ok_or(error!("account read lock failed"))?;
                if !account_data.is_writable {
                    if account_data.data[..] != existing_account_data.data[..] {
                        log_error!("ERROR: non-mutable account has been modified: {}",pubkey);
                        return Err(ErrorCode::NonMutableAccountChange.into())
                    }
                }
            }
        }
        // for account_info in accounts.lock().unwrap().iter() {
        for (pubkey, account_data) in accounts.iter() {
            // if false 
            {
                let rent = Rent::default();
                // let account_data_len = account.space;//.data_len();
                // let lamports = account_info.lamports.borrow();

                // if account_data_len == 0 && account.lamports == 0u64 { //**lamports == 0u64 {
                if account_data.data_len() == 0 && account_data.lamports == 0u64 { //**lamports == 0u64 {
                    log_trace!("{} {}",style("purging account (no data, no balance):").white().on_red(),pubkey.to_string());
                    // self.store.purge(account_info.key).await?;
                    self.store.purge(pubkey).await?;
                    continue;
                }

                let minimum_balance = rent.minimum_balance(account_data.data_len());//_data_len);
                // if **lamports < minimum_balance {
                if account_data.lamports < minimum_balance {
                    // if *account_info.key != Pubkey::default() {
                    if *pubkey != Pubkey::default() {
                        log_trace!("{} {}",style("purging account (below minimum balance):").white().on_red(),pubkey.to_string());
                        // log_trace!("data len: {} balance needed: {}  balance in the account: {}", account.space, minimum_balance, **lamports);
                        log_trace!("data len: {} balance needed: {}  balance in the account: {}", account_data.data_len(), minimum_balance, account_data.lamports);
                        log_trace!("account type: 0x{:08x}",account_data.container_type().unwrap_or(0));
                        self.store.purge(pubkey).await?;
                    }
                    // log_trace!("[store] skipping store for blank account {}", account_info.key.to_string());
                    continue;
                }
            }
            let account_data_for_storage = account_data.clone_for_storage();
            // let account_data = AccountData::clone_from_account_info(account_info);
            // log_trace!("... saving account: {} data len: {} lamports: {}  ... {}", 
            //     account_data.key.to_string(),
            //     account_data.data.len(),
            //     account_data.lamports,
            //     account_data.info(),
            // );
            log_trace!("[store] ...   saving: {}", account_data.info()?);
            self.store.store(
                &Arc::new(AccountDataReference::new(account_data_for_storage))
                // Arc::new(RwLock::new(account_data))
            ).await?;
// log_trace!("ACCOUNT DATA WRITAEABLE: {}", account_data.is_writable);
            // log_trace!("... account data: {:#?}", account_data);
            // match self.store.lookup(&account_data.key).await? {
            //     Some(existing_account_data) => {
            //         // let mut dest = account_data_reference.write()?;

            //         // let mut save = true;
            //         let existing_account_data = existing_account_data.account_data.read().await;//.ok_or(error!("account read lock failed"))?;
            //         if !account_data.is_writable {
            //             if account_data.data[..] != existing_account_data.data[..] {
            //                 log_error!("ERROR: writing to non-mutable account");
            //                 return Err(ErrorCode::NonMutableAccountChange.into())
            //                 // save = false;
            //             }
            //             // TODO: check if account was changed
            //         }
            //         if save {
            //             self.store.store(
            //                 &Arc::new(AccountDataReference::new(account_data))
            //                 // Arc::new(RwLock::new(account_data))
            //             ).await?;
            //             // self.store.store(Arc::new(RwLock::new(account_data))).await?;
            //             // *dest = account_data;
            //         }
            //     },
            //     None => {
            //         self.store.store(
            //             &Arc::new(AccountDataReference::new(account_data))
            //             // Arc::new(RwLock::new(account_data))
            //         ).await?;
            //     }
            // }

        }

        Ok(())
    }
    
    // async fn store(&self, reference : &Arc<AccountDataReference>) -> Result<()> {
    //     self.store.store(reference).await?;
    //     Ok(())
    // }


    async fn execute_impl(
        &self,
        instruction : &solana_program::instruction::Instruction
    ) -> Result<()> {

        let entrypoint = {
            match workflow_allocator::program::registry::lookup(&instruction.program_id)? {
                Some(entry_point) => { entry_point.entrypoint_fn },
                None => {
                    log_trace!("program entrypoint not found: {:?}",instruction.program_id);
                    return Err(error!("program entrypoint not found: {:?}",instruction.program_id).into());
                }
            }
        };

        let mut account_data_vec = self.clone().program_local_load(&instruction.program_id, &instruction.accounts).await?;
        {
            let mut accounts = Vec::new();
            for (pubkey, account_data) in account_data_vec.iter_mut() {
                let is_signer = account_data.is_signer;
                let is_writable = account_data.is_writable;
                let mut account_info = (&*pubkey, account_data).into_account_info();

                // pass signer and writer flags from the source account
                account_info.is_signer = is_signer;
                account_info.is_writable = is_writable;

                // accounts.lock().unwrap().push(account_info);
                accounts.push(account_info);
            }

            self.clone().execute_entrypoing_impl(&instruction.program_id, &accounts, &instruction.data, entrypoint)?;
        }

        // for (_,account) in account_data_vec.iter_mut() {
        //     // account.space = account.data_len();
        //     account.update_data_len();
        // }

        // let accounts = accounts.into_inner().unwrap
        // self.program_local_store(&accounts).await?;
        self.program_local_store(account_data_vec).await?;

        Ok(())
    }


}

#[async_trait]
impl EmulatorInterface for Emulator {

    async fn lookup(&self, pubkey: &Pubkey) -> Result<Option<Arc<AccountDataReference>>> {
        Ok(self.store.lookup(pubkey).await?)
    }

    async fn execute(
        &self,
        instruction : &solana_program::instruction::Instruction
    ) -> Result<ExecutionResponse> {

        let log_sink = self.log_sink.clone().downcast_arc::<LogSink>().expect("downcast log sink");
        log_sink.init();
        let result = self.execute_impl(instruction).await;
        let logs = log_sink.take();
        // log_trace!("logs: {:?}", logs);
        match result {
            Ok(_) => {
                Ok(ExecutionResponse::new(None, logs))
            },
            Err(err) => {
                Ok(ExecutionResponse::new(Some(err.to_string()), logs))
                // Err(err)
            }
        }

        // match  {
        //     Ok(resp) => {

        //     },
        //     Err(err) => {
        //         self.capture.store(false, Ordering::SeqCst);
        //     }
        // }
        // Ok(ExecutionResponse::new(None,None))
    }

    async fn fund(
        &self,
        // from_pubkey : &Pubkey,
        key : &Pubkey,
        owner : &Pubkey,
        lamports : u64
    ) -> Result<()> {
        
        let from = self.store.lookup(&Pubkey::default()).await?;
        let to = self.store.lookup(key).await?;

        let ref_from = if let Some(from) = from {
            from
        } else {
            return Err(error_code!(ErrorCode::LookupErrorSource));
        };

        let mut from = ref_from.account_data.write().await;
        if from.lamports < lamports {
            return Err(program_error_code!(ErrorCode::InsufficientBalance));
        }

        let ref_to = if let Some(to) = to {
            to
        } else {
            Arc::new(AccountDataReference::new(AccountData::new_static(key.clone(), owner.clone())))
        };

        let mut to = ref_to.account_data.write().await;

        from.lamports = from.lamports.saturating_sub(lamports);
        drop(from);
        to.lamports = to.lamports.saturating_add(lamports);
        drop(to);


        self.store.store(&ref_from).await?;
        self.store.store(&ref_to).await?;

        log_trace!(
            "[EMU] funding - from: {} to: {}",
            utils::shorten_pubkey(&*ref_from.key),
            utils::shorten_pubkey(&*ref_to.key),
        );

        Ok(())
    }
}


/* 
struct LogSink {
    capturing : AtomicBool,
    logs : Arc<Mutex<Vec<(Level,String)>>>
}

impl workflow_log::Sink for LogSink {
    fn write(&self, level:Level, args : &std::fmt::Arguments<'_>) -> bool {
        let text = args.to_string();
        self.logs.lock().unwrap().push((level, text));
        false
    }
}
impl LogSink {
    fn new() -> LogSink {
        LogSink {
            capturing: AtomicBool::new(false),
            logs: Arc::new(Mutex::new(Vec::new()))
        }
    }
    fn reset(&self) {
        *self.logs.lock().unwrap() = Vec::new();
    }
    fn capture(&self) {
        *self.logs.lock().unwrap() = Vec::new();
    }
}
*/