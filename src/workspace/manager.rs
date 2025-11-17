use crate::parser::{ParsedProto, ImportResolver, ProtoParser};
use anyhow::Result;
use dashmap::DashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tower_lsp::lsp_types::Url;

/// Symbol kind for protobuf elements
#[derive(Debug, Clone)]
pub enum SymbolKind {
    Message,
    Enum,
    EnumValue,
    Service,
    Method,
}

/// A symbol with its package information
#[derive(Debug, Clone)]
pub struct PackageSymbol {
    pub name: String,
    pub full_name: String,
    pub kind: SymbolKind,
    pub package: String,
}

/// Thread-safe workspace manager for caching parsed proto files
#[derive(Clone)]
pub struct WorkspaceManager {
    files: Arc<DashMap<String, Arc<ParsedProto>>>,
    resolver: Arc<parking_lot::RwLock<ImportResolver>>,
}

impl WorkspaceManager {
    pub fn new() -> Self {
        Self {
            files: Arc::new(DashMap::new()),
            resolver: Arc::new(parking_lot::RwLock::new(ImportResolver::new(vec![]))),
        }
    }

    pub fn with_additional_dirs(dirs: Vec<PathBuf>) -> Self {
        Self {
            files: Arc::new(DashMap::new()),
            resolver: Arc::new(parking_lot::RwLock::new(ImportResolver::new(dirs))),
        }
    }

    /// Opens or updates a file in the workspace
    pub async fn open_file(&self, uri: &Url, content: &str) -> Result<Arc<ParsedProto>> {
        let uri_str = uri.to_string();
        let parser = ProtoParser::new();
        let parsed: ParsedProto = parser.parse(uri_str.clone(), content).await?;
        let parsed_arc = Arc::new(parsed);
        self.files.insert(uri_str, parsed_arc.clone());
        Ok(parsed_arc)
    }

    /// Gets a parsed proto file from the cache
    pub fn get_file(&self, uri: &Url) -> Option<Arc<ParsedProto>> {
        let uri_str = uri.to_string();
        self.files.get(&uri_str).map(|entry| entry.clone())
    }

    /// Closes a file (removes from cache)
    pub fn close_file(&self, uri: &Url) {
        let uri_str = uri.to_string();
        self.files.remove(&uri_str);
    }

    /// Resolves an import from a given file
    pub fn resolve_import(&self, current_uri: &Url, import_path: &str) -> Option<PathBuf> {
        let current_path = url_to_path(current_uri)?;
        tracing::debug!("Resolving import '{}' from file: {}", import_path, current_path.display());
        let resolver = self.resolver.read();
        let resolved = resolver.resolve_import(&current_path, import_path);
        if let Some(ref path) = resolved {
            tracing::debug!("Successfully resolved to: {}", path.display());
        } else {
            tracing::debug!("Failed to resolve import");
        }
        resolved
    }

    /// Gets or loads an imported file (async version)
    pub async fn get_imported_file(&self, current_uri: &Url, import_path: &str) -> Option<Arc<ParsedProto>> {
        let resolved_path = self.resolve_import(current_uri, import_path)?;
        let import_uri = path_to_url(&resolved_path)?;

        // Check cache first
        if let Some(cached) = self.get_file(&import_uri) {
            return Some(cached);
        }

        // Try to load the file
        let content = std::fs::read_to_string(&resolved_path).ok()?;
        self.open_file(&import_uri, &content).await.ok()
    }

    /// Gets an imported file from cache only (synchronous version)
    pub fn get_imported_file_cached(&self, current_uri: &Url, import_path: &str) -> Option<Arc<ParsedProto>> {
        let resolved_path = self.resolve_import(current_uri, import_path)?;
        let import_uri = path_to_url(&resolved_path)?;

        // Only check cache
        self.get_file(&import_uri)
    }

    /// Recursively collects all imported files (including transitive imports)
    pub async fn collect_all_imports_async(&self, current_uri: &Url) -> Vec<Arc<ParsedProto>> {
        tracing::debug!("Collecting all imports for: {}", current_uri);
        let mut all_imports = Vec::new();
        let mut visited = std::collections::HashSet::new();

        if let Some(proto) = self.get_file(current_uri) {
            tracing::debug!("Current file has {} direct imports", proto.imports.len());
            for (i, import) in proto.imports.iter().enumerate() {
                tracing::debug!("  Direct import[{}]: {}", i, import.path);
            }
            self.collect_imports_recursive_async(&proto, current_uri, &mut all_imports, &mut visited).await;
        } else {
            tracing::debug!("No proto file found for URI: {}", current_uri);
        }

        tracing::debug!("Collected {} total imports", all_imports.len());
        all_imports
    }

    /// Helper function for recursive import collection
    fn collect_imports_recursive(
        &self,
        proto: &ParsedProto,
        current_uri: &Url,
        all_imports: &mut Vec<Arc<ParsedProto>>,
        visited: &mut std::collections::HashSet<String>,
    ) {
        tracing::debug!("Recursively collecting imports for {} imports", proto.imports.len());
        for import in &proto.imports {
            tracing::debug!("Attempting to resolve import: {}", import.path);
            if let Some(imported) = self.get_imported_file_cached(current_uri, &import.path) {
                let import_uri_str = imported.uri.clone();
                tracing::debug!("Successfully loaded import: {} (package: {:?})", import_uri_str, imported.package);

                // Avoid infinite recursion with circular imports
                if visited.contains(&import_uri_str) {
                    tracing::debug!("Already visited {}, skipping", import_uri_str);
                    continue;
                }
                visited.insert(import_uri_str.clone());

                all_imports.push(imported.clone());

                // Recursively collect imports from this file
                let import_url = Url::parse(&import_uri_str).ok();
                if let Some(url) = import_url {
                    self.collect_imports_recursive(&imported, &url, all_imports, visited);
                }
            } else {
                tracing::debug!("Failed to load import: {}", import.path);
            }
        }
    }

    /// Helper function for recursive import collection (async version)
    async fn collect_imports_recursive_async(
        &self,
        proto: &ParsedProto,
        current_uri: &Url,
        all_imports: &mut Vec<Arc<ParsedProto>>,
        visited: &mut std::collections::HashSet<String>,
    ) {
        tracing::debug!("Recursively collecting imports for {} imports", proto.imports.len());
        for import in &proto.imports {
            tracing::debug!("Attempting to resolve import: {}", import.path);
            // Try async loading first, then fall back to cached
            if let Some(imported) = self.get_imported_file(current_uri, &import.path).await
                .or_else(|| self.get_imported_file_cached(current_uri, &import.path)) {
                let import_uri_str = imported.uri.clone();
                tracing::debug!("Successfully loaded import: {} (package: {:?})", import_uri_str, imported.package);

                // Avoid infinite recursion with circular imports
                if visited.contains(&import_uri_str) {
                    tracing::debug!("Already visited {}, skipping", import_uri_str);
                    continue;
                }
                visited.insert(import_uri_str.clone());

                all_imports.push(imported.clone());

                // Recursively collect imports from this file
                let import_url = Url::parse(&import_uri_str).ok();
                if let Some(url) = import_url {
                    Box::pin(self.collect_imports_recursive_async(&imported, &url, all_imports, visited)).await;
                }
            } else {
                tracing::debug!("Failed to load import: {}", import.path);
            }
        }
    }

    /// Gets all symbols grouped by package name
    pub async fn get_symbols_by_package(&self, current_uri: &Url) -> std::collections::HashMap<String, Vec<PackageSymbol>> {
        let mut symbols_by_package: std::collections::HashMap<String, Vec<PackageSymbol>> = std::collections::HashMap::new();

        tracing::debug!("Getting symbols by package for URI: {}", current_uri);

        // Include current file
        if let Some(proto) = self.get_file(current_uri) {
            tracing::debug!("Current file package: {:?}", proto.package);
            self.add_symbols_from_proto(&proto, &mut symbols_by_package);
        }

        // Include all recursively imported files (async version)
        let all_imports = self.collect_all_imports_async(current_uri).await;
        tracing::debug!("Found {} imported files", all_imports.len());

        for imported in &all_imports {
            tracing::debug!("Imported file: {} (package: {:?})", imported.uri, imported.package);
            self.add_symbols_from_proto(&imported, &mut symbols_by_package);
        }

        // Log all packages and their symbol counts
        for (pkg, symbols) in &symbols_by_package {
            tracing::debug!("Package '{}' has {} symbols", pkg, symbols.len());
        }

        symbols_by_package
    }

    /// Gets all symbols grouped by package name (async version)
    pub async fn get_symbols_by_package_async(&self, current_uri: &Url) -> std::collections::HashMap<String, Vec<PackageSymbol>> {
        let mut symbols_by_package: std::collections::HashMap<String, Vec<PackageSymbol>> = std::collections::HashMap::new();

        tracing::debug!("Getting symbols by package for URI: {}", current_uri);

        // Include current file
        if let Some(proto) = self.get_file(current_uri) {
            tracing::debug!("Current file package: {:?}", proto.package);
            self.add_symbols_from_proto(&proto, &mut symbols_by_package);
        }

        // Include all recursively imported files (async version)
        let all_imports = self.collect_all_imports_async(current_uri).await;
        tracing::debug!("Found {} imported files", all_imports.len());

        for imported in &all_imports {
            tracing::debug!("Imported file: {} (package: {:?})", imported.uri, imported.package);
            self.add_symbols_from_proto(&imported, &mut symbols_by_package);
        }

        // Log all packages and their symbol counts
        for (pkg, symbols) in &symbols_by_package {
            tracing::debug!("Package '{}' has {} symbols", pkg, symbols.len());
        }

        symbols_by_package
    }

    /// Adds symbols from a proto file to the package map
    fn add_symbols_from_proto(
        &self,
        proto: &ParsedProto,
        symbols_by_package: &mut std::collections::HashMap<String, Vec<PackageSymbol>>,
    ) {
        let package_name = proto.package.clone().unwrap_or_else(|| "default".to_string());
        tracing::debug!("Processing file with package: '{}', messages: {}, enums: {}, services: {}",
            package_name, proto.messages.len(), proto.enums.len(), proto.services.len());

        let symbols = symbols_by_package.entry(package_name.clone()).or_insert_with(Vec::new);

        // Add messages
        for msg in &proto.messages {
            tracing::debug!("Adding message: {} (full: {})", msg.name, msg.full_name);
            symbols.push(PackageSymbol {
                name: msg.name.clone(),
                full_name: msg.full_name.clone(),
                kind: SymbolKind::Message,
                package: package_name.clone(),
            });
        }

        // Add enums
        for enum_ in &proto.enums {
            tracing::debug!("Adding enum: {} (full: {})", enum_.name, enum_.full_name);
            symbols.push(PackageSymbol {
                name: enum_.name.clone(),
                full_name: enum_.full_name.clone(),
                kind: SymbolKind::Enum,
                package: package_name.clone(),
            });

            // Add enum values
            for value in &enum_.values {
                symbols.push(PackageSymbol {
                    name: value.name.clone(),
                    full_name: format!("{}.{}", enum_.full_name, value.name),
                    kind: SymbolKind::EnumValue,
                    package: package_name.clone(),
                });
            }
        }

        // Add services
        for svc in &proto.services {
            tracing::debug!("Adding service: {} (full: {})", svc.name, svc.full_name);
            symbols.push(PackageSymbol {
                name: svc.name.clone(),
                full_name: svc.full_name.clone(),
                kind: SymbolKind::Service,
                package: package_name.clone(),
            });

            // Add methods
            for method in &svc.methods {
                symbols.push(PackageSymbol {
                    name: method.name.clone(),
                    full_name: format!("{}.{}", svc.full_name, method.name),
                    kind: SymbolKind::Method,
                    package: package_name.clone(),
                });
            }
        }
    }

    /// Adds an additional proto directory for import resolution
    pub fn add_proto_directory(&self, dir: PathBuf) {
        let mut resolver = self.resolver.write();
        resolver.add_directory(dir);
    }

    /// Finds a symbol across all open files
    pub fn find_symbol(&self, symbol_name: &str) -> Vec<(String, String)> {
        let mut results = Vec::new();

        for entry in self.files.iter() {
            let uri = entry.key();
            let proto = entry.value();

            // Search messages
            if let Some(_msg) = proto.find_message_by_name(symbol_name) {
                results.push((uri.clone(), "message".to_string()));
            }

            // Search enums
            if let Some(_enum) = proto.find_enum_by_name(symbol_name) {
                results.push((uri.clone(), "enum".to_string()));
            }

            // Search services
            if let Some(_svc) = proto.find_service_by_name(symbol_name) {
                results.push((uri.clone(), "service".to_string()));
            }
        }

        results
    }

    /// Gets all files in the workspace
    pub fn get_all_files(&self) -> Vec<(String, Arc<ParsedProto>)> {
        self.files
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect()
    }
}

impl Default for WorkspaceManager {
    fn default() -> Self {
        Self::new()
    }
}

fn url_to_path(url: &Url) -> Option<PathBuf> {
    url.to_file_path().ok()
}

fn path_to_url(path: &Path) -> Option<Url> {
    Url::from_file_path(path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_workspace_manager() {
        let manager = WorkspaceManager::new();
        let content = r#"
syntax = "proto3";
package test;

message Person {
    string name = 1;
}
"#;

        let url = Url::parse("file:///test/test.proto").unwrap();
        let result = manager.open_file(&url, content).await;
        assert!(result.is_ok());

        let cached = manager.get_file(&url);
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().package, Some("test".to_string()));

        manager.close_file(&url);
        assert!(manager.get_file(&url).is_none());
    }
}
