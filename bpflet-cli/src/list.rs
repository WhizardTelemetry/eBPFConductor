use anyhow::bail;

use bpflet_api::v1::{bpflet_client::BpfletClient, ListRequest};

use crate::{args::ListArgs, select_channel, table::ProgTable};

pub(crate) async fn execute_list(args: &ListArgs) -> anyhow::Result<()> {
    let channel = select_channel().unwrap();
    let mut client = BpfletClient::new(channel);
    let prog_type_filter = args.program_type.map(|p| p as u32);

    let request = tonic::Request::new(ListRequest {
        program_type: prog_type_filter,
        // Transform metadata from a vec of tuples to an owned map.
        match_metadata: args
            .metadata_selector
            .clone()
            .unwrap_or_default()
            .iter()
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect(),
        bpflet_programs_only: Some(!args.all),
    });
    let response = client.list(request).await?.into_inner();
    let mut table = ProgTable::new_list();

    for r in response.results {
        if let Err(e) = table.add_response_prog(r) {
            bail!(e)
        }
    }
    table.print();
    Ok(())
}
