pub fn fixture() -> serde_json::Value {
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!("cadcraft-proposal-pure-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    let models = ["BaseModel", "ExistingConditionsModel", "ProposedDesignModel"];
    let sheets = ["SiteOverviewSheet", "ExistingConditionsSheet", "ProposedSitePlanSheet"];
    let rect = json!({"x":0,"y":0,"width":100,"height":100});
    let mut xrefs = Vec::new();
    for host in sheets.iter().copied().chain(std::iter::once("ProposedDesignModel")) {
        for reference in models {
            if host == "ProposedDesignModel" && reference == host {
                continue;
            }
            xrefs.push(json!({"host_role":host,"reference_role":reference,"mode":"Overlay","insertion":{"x":0,"y":0,"z":0},"scale":1,"rotation":0}));
        }
    }
    json!({"version":1,"base_roots":[{"year":2026,"path":root.to_string_lossy()}],
      "folders":["Models","Sheets","Support","Shortcuts"],"model_template":"model.dxf",
      "models":models.iter().zip(["base","existing","proposed"]).map(|(role,name)|json!({"role":role,"path":format!("Models/{name}.dxf")})).collect::<Vec<_>>(),
      "sheets":sheets.iter().enumerate().map(|(i,role)|json!({"role":role,"path":format!("Sheets/{i}.dxf"),"number":format!("S{i}"),"title":role})).collect::<Vec<_>>(),
      "sheet_index_path":"Support/sheets.json","data_shortcut_path":"Shortcuts",
      "support_records":(["ProjectConfiguration","SourceRegister","AssumptionsLog","DecisionLog","CreationPlan","CreationReceipt"]).iter().map(|role|json!({"role":role,"path":format!("Support/{role}.json")})).collect::<Vec<_>>(),
      "profiles":[{"orientation":"Landscape","size":"A3","template":"sheet.dxf","layout":"Proposal","title_block":"Title","page_setup":"Proposal",
        "plot":{"device":"None","media":"A3","style_sheet":"None","units":"Millimeters","scale":1,"rotation":0},"printable_area":rect,
        "placeholders":sheets.iter().flat_map(|role| {let labels = if *role=="SiteOverviewSheet" {vec!["SITE LOCATION","SITE OVERVIEW"]} else {vec!["PLAN"]}; labels.into_iter().map(|label|json!({"sheet_role":role,"label":label,"kind":"Boundary","rectangle":rect})).collect::<Vec<_>>()}).collect::<Vec<_>>() }],
      "property_mappings":[{"input":"client_number","property":"ClientNumber","title_block_attribute":"CLIENT"}],"xrefs":xrefs})
}
