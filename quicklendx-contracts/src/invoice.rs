/// Add a tag to the invoice (Business Owner Only).
    pub fn add_tag(
        &mut self,
        env: &Env,
        tag: String,
    ) -> Result<(), crate::errors::QuickLendXError> {
        // 🔒 AUTH PROTECTION: Only the business that created the invoice can add tags.
        self.business.require_auth();

        let normalized = normalize_tag(env, &tag)?;

        if normalized.len() < 1 || normalized.len() > 50 {
            return Err(crate::errors::QuickLendXError::InvalidTag);
        }

        if self.tags.len() >= 10 {
            return Err(crate::errors::QuickLendXError::TagLimitExceeded);
        }

        for existing_tag in self.tags.iter() {
            if existing_tag == normalized {
                return Ok(());
            }
        }

        self.tags.push_back(normalized.clone());
        
        // Update Index for discoverability
        InvoiceStorage::add_tag_index(env, &normalized, &self.id);
        
        Ok(())
    }

    /// Remove a tag from the invoice (Business Owner Only).
    pub fn remove_tag(&mut self, tag: String) -> Result<(), crate::errors::QuickLendXError> {
        // 🔒 AUTH PROTECTION
        self.business.require_auth();

        let env = self.tags.env();
        let normalized = normalize_tag(&env, &tag)?;
        let mut new_tags = Vec::new(&env);
        let mut found = false;

        for existing_tag in self.tags.iter() {
            if existing_tag != normalized {
                new_tags.push_back(existing_tag.clone());
            } else {
                found = true;
            }
        }

        if !found {
            return Err(crate::errors::QuickLendXError::InvalidTag);
        }

        self.tags = new_tags;
        
        // Remove from Index
        InvoiceStorage::remove_tag_index(&env, &normalized, &self.id);
        
        Ok(())
    }

    /// Update the invoice category (Business Owner Only).
    pub fn update_category(&mut self, env: &Env, category: InvoiceCategory) -> Result<(), QuickLendXError> {
        // 🔒 AUTH PROTECTION
        self.business.require_auth();

        // 🛡️ INDEX ROLLBACK PROTECTION
        // Remove the invoice from the old category index before updating
        InvoiceStorage::remove_category_index(env, &self.category, &self.id);

        // Update the state
        self.category = category;

        // Add to the new category index
        InvoiceStorage::add_category_index(env, &self.category, &self.id);

        Ok(())
    }