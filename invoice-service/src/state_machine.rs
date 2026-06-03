use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "invoice_state", rename_all = "snake_case")]
pub enum InvoiceState {
    Draft,
    Open,
    Paid,
    Void,
    Uncollectible,
}

impl InvoiceState {
    pub fn as_str(&self) -> &'static str {
        match self {
            InvoiceState::Draft => "draft",
            InvoiceState::Open => "open",
            InvoiceState::Paid => "paid",
            InvoiceState::Void => "void",
            InvoiceState::Uncollectible => "uncollectible",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            InvoiceState::Paid | InvoiceState::Void | InvoiceState::Uncollectible
        )
    }

    pub fn can_finalize(&self) -> bool {
        *self == InvoiceState::Draft
    }

    pub fn can_void(&self) -> bool {
        matches!(self, InvoiceState::Draft | InvoiceState::Open)
    }

    pub fn can_pay(&self) -> bool {
        *self == InvoiceState::Open
    }

    pub fn transition_to(&self, next: InvoiceState) -> AppResult<InvoiceState> {
        let ok = match (self, next) {
            (InvoiceState::Draft, InvoiceState::Open) => true,
            (InvoiceState::Draft, InvoiceState::Void) => true,
            (InvoiceState::Open, InvoiceState::Paid) => true,
            (InvoiceState::Open, InvoiceState::Void) => true,
            (InvoiceState::Open, InvoiceState::Uncollectible) => true,
            (current, target) if current == &target => true,
            _ => false,
        };
        if ok {
            Ok(next)
        } else {
            Err(AppError::InvalidState(format!(
                "cannot transition invoice from {} to {}",
                self.as_str(),
                next.as_str()
            )))
        }
    }
}
