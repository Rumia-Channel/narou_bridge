#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountRotation {
    ordered_accounts: Vec<String>,
    current_index: usize,
}

impl AccountRotation {
    pub fn new(active_account: Option<&str>, accounts: impl IntoIterator<Item = String>) -> Self {
        let mut ordered_accounts = Vec::new();

        if let Some(active_account) = active_account.filter(|name| !name.is_empty()) {
            ordered_accounts.push(active_account.to_string());
        }

        for account in accounts {
            if !account.is_empty() && !ordered_accounts.iter().any(|existing| existing == &account)
            {
                ordered_accounts.push(account);
            }
        }

        Self {
            ordered_accounts,
            current_index: 0,
        }
    }

    pub fn current(&self) -> Option<&str> {
        self.ordered_accounts
            .get(self.current_index)
            .map(String::as_str)
    }

    pub fn mark_failed(&mut self) -> Option<&str> {
        if self.current_index + 1 >= self.ordered_accounts.len() {
            return None;
        }
        self.current_index += 1;
        self.current()
    }
}

#[cfg(test)]
mod tests {
    use super::AccountRotation;

    #[test]
    fn rotation_returns_accounts_in_order_with_active_first() {
        let rotation = AccountRotation::new(
            Some("login"),
            ["alpha".to_string(), "login".to_string(), "beta".to_string()],
        );

        assert_eq!(rotation.current(), Some("login"));
    }

    #[test]
    fn rotation_advances_to_next_account_after_failure() {
        let mut rotation = AccountRotation::new(
            Some("alpha"),
            ["alpha".to_string(), "beta".to_string(), "gamma".to_string()],
        );

        assert_eq!(rotation.current(), Some("alpha"));
        assert_eq!(rotation.mark_failed(), Some("beta"));
        assert_eq!(rotation.current(), Some("beta"));
        assert_eq!(rotation.mark_failed(), Some("gamma"));
        assert_eq!(rotation.mark_failed(), None);
    }
}
