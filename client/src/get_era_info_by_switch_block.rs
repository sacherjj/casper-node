use std::str;

use clap::{App, ArgMatches, SubCommand};

use casper_node::rpcs::chain::GetEraInfoBySwitchBlock;

use crate::{command::ClientCommand, common};

/// This struct defines the order in which the args are shown for this subcommand's help message.
enum DisplayOrder {
    Verbose,
    NodeAddress,
    RpcId,
    BlockIdentifier,
}

impl<'a, 'b> ClientCommand<'a, 'b> for GetEraInfoBySwitchBlock {
    const NAME: &'static str = "get-era-info-by-switch-block";
    const ABOUT: &'static str = "Retrieves era information from the network";

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
        let maybe_block_id = common::block_identifier::get(&matches);

        let response = casper_client::get_era_info_by_switch_block(
            maybe_rpc_id,
            node_address,
            verbose,
            maybe_block_id,
        )
        .unwrap_or_else(|error| panic!("response error: {}", error));
        println!(
            "{}",
            serde_json::to_string_pretty(&response).expect("should encode to JSON")
        );
    }
}
