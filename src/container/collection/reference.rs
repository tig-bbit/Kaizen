use cfg_if::cfg_if;
use crate::address::ProgramAddressData;
use crate::container::Container;
use crate::result::Result;
use workflow_allocator::prelude::*;
use workflow_allocator::error::ErrorCode;
use super::proxy::Proxy;
use super::meta::*;

pub struct AccountReferenceCollection<'info, M>{
    pub domain : &'info [u8],
    meta : M,
}

impl<'info,M> AccountReferenceCollection<'info,M>
where M: CollectionMeta
{

    fn try_create(
        domain:&'info [u8],
        mut meta:M,
        seed : &[u8],
        container_type : Option<u32>
    )->Result<Self> {
        meta.try_create(seed,container_type)?;
        Ok(Self { domain, meta })
    }

    fn try_load(
        domain:&'info [u8],
        mut meta:M,
    )->Result<Self> {
        meta.try_load()?;
        Ok(Self { domain, meta })
    }

    pub fn data_len_min() -> usize { M::min_data_len() }

    pub fn try_create_from_meta(
        data : &'info mut AccountCollectionMeta,
        account_info : &AccountInfo<'info>,
        seed : &[u8],
        container_type : Option<u32>,
    ) -> Result<AccountReferenceCollection<'info, AccountCollectionMetaReference<'info>>> {

        AccountReferenceCollection::<AccountCollectionMetaReference>::try_create(
            account_info.key.as_ref(),
            AccountCollectionMetaReference::new(data),
            seed,
            container_type,
        )
    }

    pub fn try_load_from_meta(
        data : &'info mut AccountCollectionMeta,
        account_info : &AccountInfo<'info>,
    ) -> Result<AccountReferenceCollection<'info, AccountCollectionMetaReference<'info>>> {

        AccountReferenceCollection::<AccountCollectionMetaReference>::try_load(
            account_info.key.as_ref(),
            AccountCollectionMetaReference::new(data)
        )
    }

    pub fn try_create_from_segment<'refs>(
        segment : Rc<Segment<'info, 'refs>>,
        seed : &[u8],
        container_type : Option<u32>,
    ) -> Result<AccountReferenceCollection<'info, AccountCollectionMetaSegment<'info, 'refs>>> {
        AccountReferenceCollection::<AccountCollectionMetaSegment>::try_create(
            segment.account().key.as_ref(),
            AccountCollectionMetaSegment::new(segment),
            seed,
            container_type,
        )
    }

    pub fn try_load_from_segment<'refs>(
            segment : Rc<Segment<'info, 'refs>>
    ) -> Result<AccountReferenceCollection<'info, AccountCollectionMetaSegment<'info, 'refs>>> {
        AccountReferenceCollection::<AccountCollectionMetaSegment>::try_load(
            segment.account().key.as_ref(),
            AccountCollectionMetaSegment::new(segment)
        )
    }

    pub fn len(&self) -> usize {
        self.meta.get_len() as usize
    }

    pub fn get_proxy_seed_at(&self, idx : u64) -> Vec<u8> {
        let domain = self.domain;
        let index_bytes: [u8; 8] = unsafe { std::mem::transmute(idx.to_le()) };
        [domain, &self.meta.get_seed(),&index_bytes].concat()
    }

    pub fn try_insert_reference<'refs,T>(
        &mut self,
        ctx: &ContextReference<'info,'refs,'_,'_>,
        bump : u8,
        container: &T
    )
    -> Result<()>
    where T : Container<'info,'refs>
    {
        if let Some(container_type) = self.meta.get_container_type() {
            if T::container_type() != container_type {
                return Err(error_code!(ErrorCode::ContainerTypeMismatch));
            }
        }

        let next_index = self.meta.get_len() + 1;
        let mut program_address_data_bytes = self.get_proxy_seed_at(next_index);
        program_address_data_bytes.push(bump);

        let tpl_program_address_data = ProgramAddressData::from_bytes(program_address_data_bytes.as_slice());
        let pda = Pubkey::create_program_address(
            &[tpl_program_address_data.seed],
            ctx.program_id
        )?;

        let tpl_account_info = match ctx.locate_index_account(&pda) {
            Some(account_info) => account_info,
            None => {
                return Err(error_code!(ErrorCode::AccountCollectionNotFound))
            }
        };

        let allocation_args = AccountAllocationArgs::new(AddressDomain::None);
        let account_info = ctx.try_create_pda_with_args(
            Proxy::data_len(),
            &allocation_args,
            tpl_program_address_data,
            tpl_account_info,
            false
        )?;

        Proxy::try_create(account_info, container.pubkey())?;
        self.meta.set_len(next_index);

        Ok(())
    }
    

}


cfg_if! {
    if #[cfg(not(target_arch = "bpf"))] {
        
        // use futures::join;
        // use futures::{stream::FuturesUnordered, StreamExt};
        use futures::{stream::FuturesOrdered, StreamExt};

        impl<'info,M> AccountReferenceCollection<'info,M>
        where M: CollectionMeta
        {        

        // impl<'info,'refs> AccountReferenceCollection<'info,'refs> {

            pub fn get_proxy_pda_at(&self, program_id : &Pubkey, idx : u64) -> Result<(Pubkey, u8)> {
                let (address, bump) = Pubkey::find_program_address(
                    &[&self.get_proxy_seed_at(idx)], //domain,&meta.get_seed_as_bytes(),&index_bytes],
                    program_id
                );

                Ok((address, bump))
            }


            pub fn get_proxy_pubkey_at(&self, program_id : &Pubkey, idx : usize) -> Result<Pubkey> {
                Ok(self.get_proxy_pda_at(program_id,idx as u64)?.0)
                // let meta = self.meta()?;
                // let user_seed = self.account().key.as_ref();
                // let index_bytes: [u8; 8] = unsafe { std::mem::transmute((idx as u64).to_le()) };
                // let (address, _bump_seed) = Pubkey::find_program_address(
                //     &[user_seed,&meta.seed_as_bytes(),&index_bytes],
                //     program_id
                // );

                // Ok(address)
            }
            
            // pub fn get_proxy_pubkey_seed_at(&self, program_id : &Pubkey, idx : usize) -> Result<Vec<u8>> {
    
            //     let meta = self.meta()?;

            //     let user_seed = self.account().key.as_ref();
            //     let index_bytes: [u8; 8] = unsafe { std::mem::transmute((idx as u64).to_le()) };
            //     let mut pda_seed : Vec<u8> = [meta.seed_as_bytes().as_slice(),&index_bytes].concat();

            //     let (_address, bump_seed) = Pubkey::find_program_address(
            //         &[user_seed,pda_seed.as_slice()],
            //         program_id
            //     );

            //     pda_seed.push(bump_seed);
            //     Ok(pda_seed)
            // }
            
            pub async fn load_container_at<'this,T>(&self, program_id: &Pubkey, idx: usize) 
            -> Result<Option<ContainerReference<'this,T>>> 
            where T: workflow_allocator::container::Container<'this,'this>
            {
                let transport = Transport::global()?;
                Ok(self.load_container_at_with_transport::<T>(program_id, idx, &transport).await?)
            }

            pub async fn load_container_at_with_transport<'this,T>(&self, program_id: &Pubkey, idx: usize, transport: &Arc<Transport>) 
            -> Result<Option<ContainerReference<'this,T>>> 
            where T: workflow_allocator::container::Container<'this,'this>
            {
                let proxy_pubkey = self.get_proxy_pubkey_at(program_id, idx)?;
                let proxy = match load_container_with_transport::<Proxy>(&transport, &proxy_pubkey).await? {
                    Some(proxy) => proxy,
                    None => return Err(error_code!(ErrorCode::AccountReferenceCollectionProxyNotFound))
                };

                let container_pubkey = proxy.reference();
                Ok(load_container_with_transport::<T>(&transport,container_pubkey).await?)
            }
            
    
            pub async fn load_container_range<'this,T>(&self, program_id: &Pubkey, range: std::ops::Range<usize>) 
            -> Result<Vec<Result<Option<ContainerReference<'this,T>>>>>
            where T: workflow_allocator::container::Container<'this,'this>
            {
                let transport = Transport::global()?;
                Ok(self.load_container_range_with_transport::<T>(program_id, range, &transport).await?)
            }
    
            pub async fn load_container_range_with_transport<'this,T>(&self, program_id: &Pubkey, range: std::ops::Range<usize>, transport: &Arc<Transport>) 
            -> Result<Vec<Result<Option<ContainerReference<'this,T>>>>>
            where T: workflow_allocator::container::Container<'this,'this>
            {
                let mut futures = FuturesOrdered::new();
                for idx in range {
                    let f = self.load_container_at_with_transport::<T>(program_id, idx, &transport);
                    futures.push_back(f);
                }

                Ok(futures.collect::<Vec<_>>().await)
            }

        }            

    }
}
