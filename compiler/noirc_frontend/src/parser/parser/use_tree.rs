use noirc_errors::Location;

use crate::{
    ast::{Ident, Path, PathKind, UseTree, UseTreeKind},
    parser::labels::ParsingRuleLabel,
    token::{Keyword, Token},
};

use super::{Parser, parse_many::separated_by_comma_until_right_brace};

impl Parser<'_> {
    /// Use = 'use' PathKind PathNoTurbofish UseTree
    ///
    /// UseTree = PathNoTurbofish ( '::' '{' UseTreeList? '}' )?
    ///
    /// UseTreeList = UseTree (',' UseTree)* ','?
    pub(super) fn parse_use_tree(&mut self) -> UseTree {
        let start_location = self.current_token_location;

        let kind = self.parse_path_kind();

        let use_tree = self.parse_use_tree_without_kind(
            start_location,
            kind,
            false, // nested
        );
        if !self.eat_semicolons() {
            self.expected_token(Token::Semicolon);
        }
        use_tree
    }

    pub(super) fn parse_use_tree_without_kind(
        &mut self,
        start_location: Location,
        kind: PathKind,
        nested: bool,
    ) -> UseTree {
        let prefix = self.parse_path_after_kind(
            kind,
            false, // allow turbofish
            false, // allow trailing double colon
            start_location,
        );
        let trailing_double_colon = if prefix.segments.is_empty() && kind != PathKind::Plain {
            true
        } else {
            self.eat_double_colon()
        };

        if trailing_double_colon {
            if self.eat_left_brace() {
                let use_trees = self.parse_many(
                    "use trees",
                    separated_by_comma_until_right_brace(),
                    Self::parse_use_tree_in_list,
                );

                UseTree {
                    prefix,
                    kind: UseTreeKind::List(use_trees),
                    location: self.location_since(start_location),
                }
            } else {
                self.expected_token(Token::LeftBrace);
                self.parse_path_use_tree_end(prefix, nested, start_location)
            }
        } else {
            self.parse_path_use_tree_end(prefix, nested, start_location)
        }
    }

    fn parse_use_tree_in_list(&mut self) -> Option<UseTree> {
        let start_location = self.current_token_location;

        // Special case: "self" cannot be followed by anything else
        if self.eat_self() {
            return Some(UseTree {
                prefix: Path::plain(Vec::new(), start_location),
                kind: UseTreeKind::Path(Ident::new("self".to_string(), start_location), None),
                location: start_location,
            });
        }

        let use_tree = self.parse_use_tree_without_kind(
            start_location,
            PathKind::Plain,
            true, // nested
        );

        // If we didn't advance at all, we are done
        if start_location.span == self.current_token_location.span {
            self.expected_label(ParsingRuleLabel::UseSegment);
            None
        } else {
            Some(use_tree)
        }
    }

    pub(super) fn parse_path_use_tree_end(
        &mut self,
        mut prefix: Path,
        nested: bool,
        start_location: Location,
    ) -> UseTree {
        if prefix.segments.is_empty() {
            if nested {
                self.expected_identifier();
            } else {
                self.expected_label(ParsingRuleLabel::UseSegment);
            }
            UseTree {
                prefix,
                kind: UseTreeKind::Path(self.unknown_ident_at_previous_token_end(), None),
                location: self.location_since(start_location),
            }
        } else {
            let ident = prefix.segments.pop().unwrap().ident;
            if self.eat_keyword(Keyword::As) {
                if let Some(alias) = self.eat_ident() {
                    UseTree {
                        prefix,
                        kind: UseTreeKind::Path(ident, Some(alias)),
                        location: self.location_since(start_location),
                    }
                } else {
                    self.expected_identifier();
                    UseTree {
                        prefix,
                        kind: UseTreeKind::Path(ident, None),
                        location: self.location_since(start_location),
                    }
                }
            } else {
                UseTree {
                    prefix,
                    kind: UseTreeKind::Path(ident, None),
                    location: self.location_since(start_location),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        ast::{ItemVisibility, PathKind, UseTree, UseTreeKind},
        parse_program_with_dummy_file,
        parser::{ItemKind, parser::tests::expect_no_errors},
    };

    fn parse_use_tree_no_errors(src: &str) -> (UseTree, ItemVisibility) {
        let (mut module, errors) = parse_program_with_dummy_file(src);
        expect_no_errors(&errors);
        assert_eq!(module.items.len(), 1);
        let item = module.items.remove(0);
        let ItemKind::Import(use_tree, visibility) = item.kind else {
            panic!("Expected import");
        };
        (use_tree, visibility)
    }

    #[test]
    fn parse_simple() {
        let src = "use foo;";
        let (use_tree, visibility) = parse_use_tree_no_errors(src);
        assert_eq!(visibility, ItemVisibility::Private);
        assert_eq!(use_tree.prefix.kind, PathKind::Plain);
        assert_eq!("foo", use_tree.to_string());
        let UseTreeKind::Path(ident, alias) = &use_tree.kind else {
            panic!("Expected path");
        };
        assert_eq!("foo", ident.to_string());
        assert!(alias.is_none());
    }

    #[test]
    fn parse_simple_pub() {
        let src = "pub use foo;";
        let (_use_tree, visibility) = parse_use_tree_no_errors(src);
        assert_eq!(visibility, ItemVisibility::Public);
    }

    #[test]
    fn parse_simple_pub_crate() {
        let src = "pub(crate) use foo;";
        let (_use_tree, visibility) = parse_use_tree_no_errors(src);
        assert_eq!(visibility, ItemVisibility::PublicCrate);
    }

    #[test]
    fn parse_simple_with_alias() {
        let src = "use foo as bar;";
        let (use_tree, visibility) = parse_use_tree_no_errors(src);
        assert_eq!(visibility, ItemVisibility::Private);
        assert_eq!(use_tree.prefix.kind, PathKind::Plain);
        assert_eq!("foo as bar", use_tree.to_string());
        let UseTreeKind::Path(ident, alias) = use_tree.kind else {
            panic!("Expected path");
        };
        assert_eq!("foo", ident.to_string());
        assert_eq!("bar", alias.unwrap().to_string());
    }

    #[test]
    fn parse_with_crate_prefix() {
        let src = "use crate::foo;";
        let (use_tree, visibility) = parse_use_tree_no_errors(src);
        assert_eq!(visibility, ItemVisibility::Private);
        assert_eq!(use_tree.prefix.kind, PathKind::Crate);
        assert_eq!("crate::foo", use_tree.to_string());
        let UseTreeKind::Path(ident, alias) = use_tree.kind else {
            panic!("Expected path");
        };
        assert_eq!("foo", ident.to_string());
        assert!(alias.is_none());
    }

    #[test]
    fn parse_with_dep_prefix() {
        let src = "use dep::foo;";
        let (use_tree, visibility) = parse_use_tree_no_errors(src);
        assert_eq!(visibility, ItemVisibility::Private);
        assert_eq!(use_tree.prefix.kind, PathKind::Dep);
        assert_eq!("dep::foo", use_tree.to_string());
        let UseTreeKind::Path(ident, alias) = use_tree.kind else {
            panic!("Expected path");
        };
        assert_eq!("foo", ident.to_string());
        assert!(alias.is_none());
    }

    #[test]
    fn parse_with_super_prefix() {
        let src = "use super::foo;";
        let (use_tree, visibility) = parse_use_tree_no_errors(src);
        assert_eq!(visibility, ItemVisibility::Private);
        assert_eq!(use_tree.prefix.kind, PathKind::Super);
        assert_eq!("super::foo", use_tree.to_string());
        let UseTreeKind::Path(ident, alias) = use_tree.kind else {
            panic!("Expected path");
        };
        assert_eq!("foo", ident.to_string());
        assert!(alias.is_none());
    }

    #[test]
    fn parse_list() {
        let src = "use foo::{bar, baz};";
        let (use_tree, visibility) = parse_use_tree_no_errors(src);
        assert_eq!(visibility, ItemVisibility::Private);
        assert_eq!(use_tree.prefix.kind, PathKind::Plain);
        assert_eq!("foo::{bar, baz}", use_tree.to_string());
        let UseTreeKind::List(use_trees) = &use_tree.kind else {
            panic!("Expected list");
        };
        assert_eq!(use_trees.len(), 2);
    }

    #[test]
    fn parse_list_trailing_comma() {
        let src = "use foo::{bar, baz, };";
        let (use_tree, visibility) = parse_use_tree_no_errors(src);
        assert_eq!(visibility, ItemVisibility::Private);
        assert_eq!(use_tree.prefix.kind, PathKind::Plain);
        assert_eq!("foo::{bar, baz}", use_tree.to_string());
        let UseTreeKind::List(use_trees) = &use_tree.kind else {
            panic!("Expected list");
        };
        assert_eq!(use_trees.len(), 2);
    }

    #[test]
    fn parse_list_that_starts_with_crate() {
        let src = "use crate::{foo, bar};";
        let (use_tree, visibility) = parse_use_tree_no_errors(src);
        assert_eq!(visibility, ItemVisibility::Private);
        assert_eq!("crate::{foo, bar}", use_tree.to_string());
    }

    #[test]
    fn errors_on_crate_in_subtree() {
        let src = "use foo::{crate::bar}";
        let (_, errors) = parse_program_with_dummy_file(src);
        assert!(!errors.is_empty());
    }

    #[test]
    fn errors_on_double_colon_after_self() {
        let src = "use foo::{self::bar};";
        let (_, errors) = parse_program_with_dummy_file(src);
        assert!(!errors.is_empty());
    }
}
