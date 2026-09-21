use crate::budget::MapBudgetPlan;

pub fn format_aider(plan: &MapBudgetPlan) -> String {
    if plan.files.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    for (i, file) in plan.files.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&format!("{}:\n", file.path.display()));

        let mut defs = file.definitions.clone();
        defs.sort_by_key(|d| d.line);
        for def in defs {
            out.push_str("│⋮...\n│");
            out.push_str(&def.signature);
            out.push('\n');
        }
    }
    out
}
