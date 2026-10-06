//! Builds [`MirresDetails`] from a Mirres project. Child of `mirres.rs`.
//! Prices (`fixed_price`, `rate_table`) and phone numbers are never read.

use super::{Person, Project};
use crate::tempo_line_contract::{MirresDetails, MirresPerson};

fn person(p: Option<&Person>) -> Option<MirresPerson> {
    let p = p.filter(|p| p.active != Some(false))?;
    let name = p.name.as_deref().map(str::trim).filter(|n| !n.is_empty())?;
    Some(MirresPerson {
        name: name.to_owned(),
        email: p.email.clone().filter(|e| !e.trim().is_empty()),
    })
}

pub(super) fn details(p: &Project) -> Option<MirresDetails> {
    let inc = p.included_hours.as_ref();
    let d = MirresDetails {
        customer_name: p.customer.as_ref().and_then(|c| c.name.clone()),
        owner: person(p.owner.as_ref()),
        responsible: person(p.customer.as_ref().and_then(|c| c.responsible.as_ref())),
        team_lead: person(p.team.as_ref().and_then(|t| t.lead.as_ref())),
        period: inc.and_then(|i| i.period.clone()),
        allowance_hours: inc.and_then(|i| i.allowance_hours),
        used_hours: inc.and_then(|i| i.used_hours),
        remaining_hours: inc.and_then(|i| i.remaining_hours),
        usage_status: inc.and_then(|i| i.usage_status.clone()),
        due_date: p.due_date.clone(),
        contract_url: p.contract_url.clone(),
    };
    let empty = MirresDetails {
        customer_name: None,
        owner: None,
        responsible: None,
        team_lead: None,
        period: None,
        allowance_hours: None,
        used_hours: None,
        remaining_hours: None,
        usage_status: None,
        due_date: None,
        contract_url: None,
    };
    (d != empty).then_some(d)
}
