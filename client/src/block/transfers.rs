use std::str;

use clap::{App, ArgMatches, SubCommand};

use casper_node::rpcs::chain::GetBlockTransfers;

use crate::{command::ClientCommand, common};

/// This struct defines the order in which the args are shown for this subcommand.
enum DisplayOrder {
    Verbose,
    NodeAddress,
    RpcId,
    BlockIdentifier,
}

impl<'a, 'b> ClientCommand<'a, 'b> for GetBlockTransfers {
    const NAME: &'static str = "get-block-transfers";
    const ABOUT: &'static str = "Retrieves all transfers for a block from the network";

    fn build(display_order: usize) -> App<'a, 'b> {
        SubCommand::with_name(Self::NAME)
            .about(Self::ABOUT)
            .display_order(display_order)
            .arg(common::verbose::arg(DisplayOrder::Verbose as usize))
            .arg(common::node_address::arg(
                DisplayOrder::NodeAddress as usize,
            ))
            .arg(common::rpc_id::arg(DisplayOrder::RpcId as usize))
            .arg(common::block_identifier::arg(
                DisplayOrder::BlockIdentifier as usize,
            ))
    }

    fn run(matches: &ArgMatches<'_>) {
        let maybe_rpc_id = common::rpc_id::get(matches);
        let node_address = common::node_address::get(matches);
        let verbose = common::verbose::get(matches);
        let maybe_block_id = common::block_identifier::get(matches);

        let response =
            casper_client::get_block_transfers(maybe_rpc_id, node_address, verbose, maybe_block_id)
                .unwrap_or_else(|error| panic!("response error: {}", error));
        println!(
            "{}",
            serde_json::to_string_pretty(&response).expect("should encode to JSON")
        );
    }
}
