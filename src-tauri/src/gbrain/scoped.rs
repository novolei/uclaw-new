#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnowledgeScope {
    RoomOnly,
    RoomPlusPlatform,
    Global,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GbrainRoomScope {
    pub platform: String,
    pub room_id: String,
    pub knowledge_scope: KnowledgeScope,
}

impl GbrainRoomScope {
    pub fn new(
        platform: &str,
        room_id: &str,
        knowledge_scope: KnowledgeScope,
    ) -> Result<Self, String> {
        let platform = sanitize_segment(platform);
        let room_id = sanitize_segment(room_id);
        if platform.is_empty() || room_id.is_empty() {
            return Err("invalid_gbrain_room_scope".to_string());
        }
        Ok(Self {
            platform,
            room_id,
            knowledge_scope,
        })
    }

    pub fn room_prefix(&self) -> String {
        format!("live/{}/{}/", self.platform, self.room_id)
    }

    pub fn allowed_prefixes(&self) -> Vec<String> {
        if self.knowledge_scope == KnowledgeScope::Global {
            return Vec::new();
        }
        let mut prefixes = vec![self.room_prefix()];
        if self.knowledge_scope == KnowledgeScope::RoomPlusPlatform {
            prefixes.push(format!("live/{}/shared/", self.platform));
        }
        prefixes
    }

    pub fn scoped_slug(&self, slug: &str) -> Result<String, String> {
        let slug = sanitize_slug(slug);
        if slug.is_empty() {
            return Err("invalid_gbrain_page_slug".to_string());
        }
        if self.knowledge_scope == KnowledgeScope::Global {
            return Ok(slug);
        }
        let full_slug = format!("{}{}", self.room_prefix(), slug);
        self.validate_slug(&full_slug)?;
        Ok(full_slug)
    }

    pub fn validate_slug(&self, slug: &str) -> Result<(), String> {
        if self.knowledge_scope == KnowledgeScope::Global {
            return Ok(());
        }
        if self
            .allowed_prefixes()
            .iter()
            .any(|prefix| slug.starts_with(prefix))
        {
            Ok(())
        } else {
            Err("gbrain_slug_out_of_room_scope".to_string())
        }
    }
}

fn sanitize_segment(value: &str) -> String {
    value
        .chars()
        .filter_map(|ch| {
            if ch.is_ascii_alphanumeric() {
                Some(ch.to_ascii_lowercase())
            } else if ch == '-' || ch == '_' {
                Some('-')
            } else {
                None
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn sanitize_slug(value: &str) -> String {
    value
        .split('/')
        .map(sanitize_segment)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_prefix_is_platform_and_room_scoped() {
        let scope = GbrainRoomScope::new("douyin", "room-123", KnowledgeScope::RoomOnly).unwrap();
        assert_eq!(scope.room_prefix(), "live/douyin/room-123/");
        assert_eq!(scope.allowed_prefixes(), vec!["live/douyin/room-123/"]);
    }

    #[test]
    fn shared_prefix_requires_explicit_scope() {
        let scope =
            GbrainRoomScope::new("douyin", "room-123", KnowledgeScope::RoomPlusPlatform).unwrap();
        assert_eq!(
            scope.allowed_prefixes(),
            vec!["live/douyin/room-123/", "live/douyin/shared/"]
        );
    }

    #[test]
    fn rejects_unscoped_page_slug() {
        let scope = GbrainRoomScope::new("douyin", "room-123", KnowledgeScope::RoomOnly).unwrap();
        assert!(scope.validate_slug("projects/private").is_err());
    }

    #[test]
    fn global_scope_allows_unscoped_page_slugs_for_testing() {
        let scope = GbrainRoomScope::new("douyin", "room-123", KnowledgeScope::Global).unwrap();
        assert_eq!(scope.allowed_prefixes(), Vec::<String>::new());
        assert!(scope.validate_slug("projects/private").is_ok());
        assert_eq!(scope.scoped_slug("Projects/Private").unwrap(), "projects/private");
    }

    #[test]
    fn scoped_slug_is_forced_under_room_prefix() {
        let scope = GbrainRoomScope::new("DouYin", "Room 123", KnowledgeScope::RoomOnly).unwrap();
        assert_eq!(
            scope.scoped_slug("Questions/FAQ").unwrap(),
            "live/douyin/room123/questions/faq"
        );
    }
}
