#![allow(unused)]

use base64;
use snafu::{Backtrace, OptionExt, ResultExt, Snafu};
use std::collections::HashMap;
use std::convert::TryFrom;
use tokio::sync::mpsc::{channel, Receiver, Sender};

/// Shorthand for result types returned from the Simulation simulation.
pub type Result<V, E = Error> = std::result::Result<V, E>;

/// Errors arising from the simulation.
#[derive(Debug, Snafu)]
pub enum Error {
    Decode {
        backtrace: Backtrace,
        source: base64::DecodeError,
    },
    OutOfBounds {
        message: String,
    },
    /// Operation was cancelled because the simulation is shutting down.
    Terminated,
}

#[derive(Debug)]
enum Operation {
    CreateExecutionEnvironment(args::CreateExecutionEnvironment, Sender<Result<u32>>),
    CreateShardBlock(args::CreateShardBlock, Sender<Result<u32>>),
    CreateShardChain(args::CreateShardChain, Sender<u32>),
    GetExecutionEnvironment(
        args::GetExecutionEnvironment,
        Sender<Result<args::ExecutionEnvironment>>,
    ),
    GetShardBlock(args::GetShardBlock, Sender<Result<args::ShardBlock>>),
    GetSimulationState(args::GetSimulationState, Sender<args::SimulationState>),
}

#[derive(Debug)]
pub struct Handle {
    receiver: Receiver<Operation>,
    sender: Sender<Operation>,
    simulation: Simulation,
}

impl Handle {
    pub fn new(simulation: Simulation) -> Self {
        let (sender, receiver) = channel(1);

        Self {
            receiver,
            sender,
            simulation,
        }
    }

    pub async fn run(mut self) -> Result<()> {
        eprintln!("Simulation Running: {:?}", std::thread::current().id());
        while let Some(op) = self.receiver.recv().await {
            match op {
                Operation::CreateExecutionEnvironment(args, mut reply) => {
                    let res = self.simulation.create_execution_environment(args);
                    reply.send(res).await;
                }
                Operation::CreateShardBlock(args, mut reply) => {
                    let res = self.simulation.create_shard_block(args);
                    reply.send(res).await;
                }
                Operation::CreateShardChain(args, mut reply) => {
                    let res = self.simulation.create_shard_chain(args);
                    reply.send(res).await;
                }
                Operation::GetExecutionEnvironment(args, mut reply) => {
                    let res = self.simulation.get_execution_environment(args);
                    reply.send(res).await;
                }
                Operation::GetShardBlock(args, mut reply) => {
                    let res = self.simulation.get_shard_block(args);
                    reply.send(res).await;
                }
                Operation::GetSimulationState(args, mut reply) => {
                    let res = self.simulation.simulation_state(args);
                    reply.send(res).await;
                }
            }
        }

        Ok(())
    }

    // TODO: could combine the results here (and in other methods) so it just returns a single Result<> value...
    pub async fn create_execution_environment(
        &mut self,
        arg: args::CreateExecutionEnvironment,
    ) -> Result<Result<u32>> {
        let (sender, mut receiver) = channel(1);

        self.sender
            .send(Operation::CreateExecutionEnvironment(arg, sender))
            .await;

        receiver.recv().await.context(Terminated)
    }

    pub async fn create_shard_block(&mut self, arg: args::CreateShardBlock) -> Result<Result<u32>> {
        let (sender, mut receiver) = channel(1);

        self.sender
            .send(Operation::CreateShardBlock(arg, sender))
            .await;

        receiver.recv().await.context(Terminated)
    }

    pub async fn create_shard_chain(&mut self, arg: args::CreateShardChain) -> Result<u32> {
        let (sender, mut receiver) = channel(1);

        self.sender
            .send(Operation::CreateShardChain(arg, sender))
            .await;

        receiver.recv().await.context(Terminated)
    }

    pub async fn execution_environment(
        &mut self,
        arg: args::GetExecutionEnvironment,
    ) -> Result<Result<args::ExecutionEnvironment>> {
        let (sender, mut receiver) = channel(1);

        self.sender
            .send(Operation::GetExecutionEnvironment(arg, sender))
            .await;

        receiver.recv().await.context(Terminated)
    }

    pub async fn shard_block(
        &mut self,
        arg: args::GetShardBlock,
    ) -> Result<Result<args::ShardBlock>> {
        let (sender, mut receiver) = channel(1);

        self.sender
            .send(Operation::GetShardBlock(arg, sender))
            .await;

        receiver.recv().await.context(Terminated)
    }

    pub async fn simulation_state(
        &mut self,
        arg: args::GetSimulationState,
    ) -> Result<args::SimulationState> {
        let (sender, mut receiver) = channel(1);

        self.sender
            .send(Operation::GetSimulationState(arg, sender))
            .await;

        receiver.recv().await.context(Terminated)
    }
}

#[derive(Debug)]
pub struct Simulation {
    beacon_chain: BeaconChain,
    shard_chains: Vec<ShardChain>,
}

impl Simulation {
    pub fn new() -> Self {
        Self {
            beacon_chain: BeaconChain::new(),
            shard_chains: Vec::new(),
        }
    }

    pub fn simulation_state(&self, args: args::GetSimulationState) -> args::SimulationState {
        args::SimulationState {
            num_execution_environments: self.beacon_chain.execution_environments.len() as u32,
            num_shard_chains: self.shard_chains.len() as u32,
        }
    }

    /// Creates a new execution environment on the BeaconChain and returns the
    /// index of the created execution environment
    pub fn create_execution_environment(
        &mut self,
        args: args::CreateExecutionEnvironment,
    ) -> Result<u32> {
        let execution_environment = ExecutionEnvironment::try_from(args.execution_environment)?;
        let EeIndex(ee_index) = self
            .beacon_chain
            .add_execution_environment(execution_environment);
        Ok(ee_index)
    }

    pub fn get_execution_environment(
        &self,
        args: args::GetExecutionEnvironment,
    ) -> Result<args::ExecutionEnvironment> {
        if let Some(execution_environment) = self
            .beacon_chain
            .execution_environments
            .get(args.execution_environment_index as usize)
        {
            Ok(args::ExecutionEnvironment::from(execution_environment))
        } else {
            Err(Error::OutOfBounds {
                message: format!(
                    "No execution environment exists at index: {}",
                    args.execution_environment_index
                ),
            })
        }
    }

    /// Returns the index of the newly added shard chain
    /// Longer-term, can accept a config here
    pub fn create_shard_chain(&mut self, _: args::CreateShardChain) -> u32 {
        let shard_chain = ShardChain::new();
        self.shard_chains.push(shard_chain);
        (self.shard_chains.len() - 1) as u32
    }

    /// Creates a new shard block and returns the
    /// index of the created shard block
    pub fn create_shard_block(&mut self, args: args::CreateShardBlock) -> Result<u32> {
        if let Some(shard_chain) = self.shard_chains.get_mut(args.shard_chain_index as usize) {
            let shard_block = ShardBlock::try_from(args.shard_block)?;

            // TODO: Run each transaction (which will update the EE state for that shard)
            // Questions to answer:
            //   * What if the decoding of the base64 data string fails? Remove this transaction from the block?  Send back error value as result?
            //   * What if executing the EE code fails with the given data? (Same options as above?)
            // Example code from previous brainstorm:
            //        let transactions = shard_block.transactions
            //
            //        for transaction in shard_block.transactions {
            //            // This executes everything and presumably also updates the EE State on the shard
            //            let ee = transaction.execution_environment;
            //            let input_data = transaction.data;
            //
            //            let code = self.beacon_chain.get(ee);
            //            let runtime = RootRuntime::new(&code, shard_ee_state_or_something_similar);
            //            runtime.execute(input_data);
            //        }

            shard_chain.shard_blocks.push(shard_block);
            Ok((shard_chain.shard_blocks.len() - 1) as u32)
        } else {
            Err(Error::OutOfBounds {
                message: format!("No shard chain exists at index: {}", args.shard_chain_index),
            })
        }
    }

    pub fn get_shard_block(&self, args: args::GetShardBlock) -> Result<args::ShardBlock> {
        if let Some(shard_chain) = self.shard_chains.get(args.shard_chain_index as usize) {
            if let Some(shard_block) = shard_chain
                .shard_blocks
                .get(args.shard_block_index as usize)
            {
                Ok(args::ShardBlock::from(shard_block))
            } else {
                Err(Error::OutOfBounds {
                    message: format!(
                        "the shard chain at index '{}' does not contain a block at index '{}'",
                        args.shard_chain_index, args.shard_block_index
                    ),
                })
            }
        } else {
            Err(Error::OutOfBounds {
                message: format!(
                    "no shard chain exists at index '{}'",
                    args.shard_chain_index
                ),
            })
        }
    }
}

pub mod args {

    // Incoming argument values

    #[derive(Debug, Default)]
    pub struct GetSimulationState {}
    #[derive(Debug, Default)]
    pub struct CreateExecutionEnvironment {
        pub execution_environment: ExecutionEnvironment,
    }
    #[derive(Debug, Default)]
    pub struct GetExecutionEnvironment {
        pub execution_environment_index: u32,
    }
    #[derive(Debug, Default)]
    pub struct CreateShardChain {}
    #[derive(Debug, Default)]
    pub struct CreateShardBlock {
        pub shard_chain_index: u32,
        pub shard_block: ShardBlock,
    }
    #[derive(Debug, Default)]
    pub struct GetShardBlock {
        pub shard_chain_index: u32,
        pub shard_block_index: u32,
    }

    // Return values AND/OR sub-components of incoming argument values

    #[derive(Debug, Default)]
    pub struct ExecutionEnvironment {
        pub base64_encoded_wasm_code: String,
    }

    impl From<&super::ExecutionEnvironment> for ExecutionEnvironment {
        fn from(ee: &super::ExecutionEnvironment) -> Self {
            let base64_encoded_wasm_code = base64::encode(&ee.wasm_code);
            Self {
                base64_encoded_wasm_code,
            }
        }
    }

    #[derive(Debug, Default)]
    pub struct SimulationState {
        pub num_execution_environments: u32,
        pub num_shard_chains: u32,
    }
    #[derive(Debug, Default, Eq, PartialEq)]
    pub struct ShardBlock {
        pub transactions: Vec<ShardTransaction>,
    }
    impl From<&super::ShardBlock> for ShardBlock {
        fn from(sb: &super::ShardBlock) -> Self {
            let transactions: Vec<ShardTransaction> = sb
                .transactions
                .iter()
                .map(|st| -> ShardTransaction { ShardTransaction::from(st) })
                .collect();
            ShardBlock { transactions }
        }
    }

    #[derive(Debug, Default, Eq, PartialEq)]
    pub struct ShardTransaction {
        pub base64_encoded_data: String,
        pub ee_index: u32,
    }

    impl From<&super::ShardTransaction> for ShardTransaction {
        fn from(st: &super::ShardTransaction) -> Self {
            let base64_encoded_data = base64::encode(&st.data);
            let super::EeIndex(ee_index) = st.ee_index;
            Self {
                base64_encoded_data,
                ee_index,
            }
        }
    }
}

#[derive(Debug, Default)]
struct BeaconChain {
    // There are an unbounded number of EEs that can "exist" on the beacon chain
    execution_environments: Vec<ExecutionEnvironment>,
}

impl BeaconChain {
    fn new() -> Self {
        Self {
            execution_environments: Vec::new(),
        }
    }

    // Adds a new execution environment, returns the index of that new EE
    fn add_execution_environment(
        &mut self,
        execution_environment: ExecutionEnvironment,
    ) -> EeIndex {
        self.execution_environments.push(execution_environment);
        EeIndex((self.execution_environments.len() - 1) as u32)
    }
}

#[derive(Default, Debug)]
struct ShardChain {
    // Longer-term, we may need to worry about rollbacks / "staging" changes to these before committing
    // (maybe not, but worth keeping in mind that could be an issue)
    execution_environment_state: HashMap<EeIndex, ExecutionEnvironmentState>,
    shard_blocks: Vec<ShardBlock>,
}

impl ShardChain {
    fn new() -> Self {
        Self {
            execution_environment_state: HashMap::new(),
            shard_blocks: Vec::new(),
        }
    }
}

#[derive(Debug, Default, Hash, Clone, Copy, Eq, PartialEq)]
pub struct EeIndex(u32);

// The execution environment data that lives on the beacon chain
// Does NOT include shard-specific EE state
#[derive(Debug)]
struct ExecutionEnvironment {
    wasm_code: Vec<u8>,
}

impl TryFrom<args::ExecutionEnvironment> for ExecutionEnvironment {
    type Error = Error;
    fn try_from(ee_args: args::ExecutionEnvironment) -> Result<Self, Self::Error> {
        let wasm_code = base64::decode(&ee_args.base64_encoded_wasm_code).context(Decode)?;
        Ok(Self { wasm_code })
    }
}

// The execution environment state that lives on each shard chain
#[derive(Debug)]
struct ExecutionEnvironmentState {
    data: [u8; 32],
}

#[derive(Debug)]
struct ShardBlock {
    transactions: Vec<ShardTransaction>,
}

impl ShardBlock {
    fn new(transactions: Vec<ShardTransaction>) -> Self {
        Self { transactions }
    }
    fn add_transaction(&mut self, transaction: ShardTransaction) {
        self.transactions.push(transaction);
    }
}
impl TryFrom<args::ShardBlock> for ShardBlock {
    type Error = Error;
    fn try_from(sb_args: args::ShardBlock) -> Result<Self, Self::Error> {
        let transactions: Result<Vec<ShardTransaction>> = sb_args
            .transactions
            .iter()
            .map(|sbt_args| -> Result<ShardTransaction> { ShardTransaction::try_from(sbt_args) })
            .collect();
        match transactions {
            Err(e) => Err(e),
            Ok(transactions) => Ok(ShardBlock { transactions }),
        }
    }
}

#[derive(Default, Debug)]
struct ShardTransaction {
    data: Vec<u8>,
    ee_index: EeIndex,
}
impl TryFrom<&args::ShardTransaction> for ShardTransaction {
    type Error = Error;
    fn try_from(sbt_args: &args::ShardTransaction) -> Result<Self, Self::Error> {
        let data = base64::decode(&sbt_args.base64_encoded_data).context(Decode)?;
        let ee_index = EeIndex(sbt_args.ee_index);
        Ok(Self { data, ee_index })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn can_create_and_get_execution_environments() {
        let mut eth = Simulation::new();

        // Can create a new EE
        let example_wasm_code = "some wasm code here";
        let ee_args = args::ExecutionEnvironment {
            base64_encoded_wasm_code: base64::encode(example_wasm_code),
        };
        let create_ee_args = args::CreateExecutionEnvironment {
            execution_environment: ee_args,
        };
        let result = eth.create_execution_environment(create_ee_args).unwrap();
        assert_eq!(
            result, 0,
            "The first execution environment created should have an index of 0"
        );

        // Can retrieve the newly-created EE
        let get_ee_args = args::GetExecutionEnvironment {
            execution_environment_index: result,
        };
        let ee_args_retrieved = eth.get_execution_environment(get_ee_args).unwrap();
        assert_eq!(
            ee_args_retrieved.base64_encoded_wasm_code,
            base64::encode(example_wasm_code),
            "EE wasm code retrieved should match the EE wasm code that was created"
        );

        // Can create and retrieve a second EE
        let example_wasm_code = "some other wasm code here";
        let ee_args = args::ExecutionEnvironment {
            base64_encoded_wasm_code: base64::encode(example_wasm_code),
        };
        let create_ee_args = args::CreateExecutionEnvironment {
            execution_environment: ee_args,
        };
        let result = eth.create_execution_environment(create_ee_args).unwrap();
        assert_eq!(
            result, 1,
            "The second execution environment created should have an index of 1"
        );
        let get_ee_args = args::GetExecutionEnvironment {
            execution_environment_index: result,
        };
        let ee_args_retrieved = eth.get_execution_environment(get_ee_args).unwrap();
        assert_eq!(
            ee_args_retrieved.base64_encoded_wasm_code,
            base64::encode(example_wasm_code),
            "EE wasm code retrieved should match the EE wasm code that was created"
        );
    }
    #[test]
    fn getting_ee_at_incorrect_index_should_return_err() {
        let mut eth = Simulation::new();
        let get_ee_args = args::GetExecutionEnvironment {
            execution_environment_index: 155512,
        };
        let ee_args_retrieved = eth.get_execution_environment(get_ee_args);
        assert!(ee_args_retrieved.is_err());
    }
    #[test]
    fn can_create_shard_chains() {
        let mut eth = Simulation::new();
        let sc_args = args::CreateShardChain {};
        let result = eth.create_shard_chain(sc_args);
        assert_eq!(
            result, 0,
            "The first shard chain created should have an index of 0"
        );

        let sc_args = args::CreateShardChain {};
        let result = eth.create_shard_chain(sc_args);
        assert_eq!(
            result, 1,
            "The second shard chain created should have an index of 1"
        );
    }
    #[test]
    fn can_get_simulation_state() {
        let mut eth = Simulation::new();

        let get_ss_args = args::GetSimulationState {};
        let general_state = eth.simulation_state(get_ss_args);
        assert_eq!(0, general_state.num_shard_chains);
        assert_eq!(0, general_state.num_execution_environments);

        let sc_args = args::CreateShardChain {};
        eth.create_shard_chain(sc_args);

        let get_ss_args = args::GetSimulationState {};
        let general_state = eth.simulation_state(get_ss_args);
        assert_eq!(1, general_state.num_shard_chains);
        assert_eq!(0, general_state.num_execution_environments);

        let ee_args = args::ExecutionEnvironment {
            base64_encoded_wasm_code: base64::encode("wasm msaw"),
        };
        let create_ee_args = args::CreateExecutionEnvironment {
            execution_environment: ee_args,
        };
        eth.create_execution_environment(create_ee_args);
        let get_ss_args = args::GetSimulationState {};
        let general_state = eth.simulation_state(get_ss_args);
        assert_eq!(1, general_state.num_shard_chains);
        assert_eq!(1, general_state.num_execution_environments);
    }

    fn create_example_shard_block_args(ee_index: u32) -> args::ShardBlock {
        // Create transaction arguments
        let transaction_args1 = args::ShardTransaction {
            base64_encoded_data: base64::encode("some data"),
            ee_index,
        };
        let transaction_args2 = args::ShardTransaction {
            base64_encoded_data: base64::encode("some other data"),
            ee_index,
        };

        // Create shard block arguments
        let sb_args = args::ShardBlock {
            transactions: vec![transaction_args1, transaction_args2],
        };

        sb_args
    }
    #[test]
    fn can_create_and_get_shard_blocks() {
        let mut eth = Simulation::new();

        // Add EE
        let example_wasm_code = "some wasm code here";
        let ee_args = args::ExecutionEnvironment {
            base64_encoded_wasm_code: base64::encode(example_wasm_code),
        };
        let create_ee_args = args::CreateExecutionEnvironment {
            execution_environment: ee_args,
        };
        let ee_index = eth.create_execution_environment(create_ee_args).unwrap();

        // Add Shard Chain
        let sc_args = args::CreateShardChain {};
        let sc_index = eth.create_shard_chain(sc_args);

        // Create shard block args
        let sb_args1 = create_example_shard_block_args(ee_index);
        let sb_args2 = create_example_shard_block_args(ee_index);

        // Add shard blocks and assert that indices look correct
        let create_shard_block_args1 = args::CreateShardBlock {
            shard_chain_index: sc_index,
            shard_block: sb_args1,
        };
        let create_shard_block_args2 = args::CreateShardBlock {
            shard_chain_index: sc_index,
            shard_block: sb_args2,
        };
        let block_index1 = eth.create_shard_block(create_shard_block_args1).unwrap();
        let block_index2 = eth.create_shard_block(create_shard_block_args2).unwrap();
        assert_eq!(
            block_index1, 0,
            "first shard block added should have index of 0"
        );
        assert_eq!(
            block_index2, 1,
            "second shard block added should have index of 1"
        );

        // Get back shard blocks and make sure they look the same as originally
        let get_shard_block_args1 = args::GetShardBlock {
            shard_chain_index: sc_index,
            shard_block_index: block_index1,
        };
        let mut sb_args_returned = eth.get_shard_block(get_shard_block_args1).unwrap();
        assert_eq!(
            sb_args_returned,
            create_example_shard_block_args(ee_index),
            "value saved should match initial args passed in"
        );

        let get_shard_block_args2 = args::GetShardBlock {
            shard_chain_index: sc_index,
            shard_block_index: block_index2,
        };
        let mut sb_args_returned = eth.get_shard_block(get_shard_block_args2).unwrap();
        assert_eq!(
            sb_args_returned,
            create_example_shard_block_args(ee_index),
            "value saved should match initial args passed in"
        );
    }
}