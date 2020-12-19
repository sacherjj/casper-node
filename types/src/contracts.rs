//! Data types for supporting contract headers feature.

use crate::{
    alloc::string::ToString,
    bytesrepr::{self, FromBytes, ToBytes, U32_SERIALIZED_LENGTH},
    uref::URef,
    CLType, ContractHash, ContractPackageHash, ContractWasmHash, Key, ProtocolVersion,
    KEY_HASH_LENGTH,
};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::String,
    vec::Vec,
};
use core::fmt;

/// Maximum number of distinct user groups.
pub const MAX_GROUPS: u8 = 10;
/// Maximum number of URefs which can be assigned across all user groups.
pub const MAX_TOTAL_UREFS: usize = 100;

/// Set of errors which may happen when working with contract headers.
#[derive(Debug, PartialEq)]
#[repr(u8)]
pub enum Error {
    /// Attempt to override an existing or previously existing version with a
    /// new header (this is not allowed to ensure immutability of a given
    /// version).
    PreviouslyUsedVersion = 1,
    /// Attempted to disable a contract that does not exist.
    ContractNotFound = 2,
    /// Attempted to create a user group which already exists (use the update
    /// function to change an existing user group).
    GroupAlreadyExists = 3,
    /// Attempted to add a new user group which exceeds the allowed maximum
    /// number of groups.
    MaxGroupsExceeded = 4,
    /// Attempted to add a new URef to a group, which resulted in the total
    /// number of URefs across all user groups to exceed the allowed maximum.
    MaxTotalURefsExceeded = 5,
    /// Attempted to remove a URef from a group, which does not exist in the
    /// group.
    GroupDoesNotExist = 6,
    /// Attempted to remove unknown URef from the group.
    UnableToRemoveURef = 7,
    /// Group is use by at least one active contract.
    GroupInUse = 8,
    /// URef already exists in given group.
    URefAlreadyExists = 9,
}

/// A (labelled) "user group". Each method of a versioned contract may be
/// assoicated with one or more user groups which are allowed to call it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Group(String);

impl Group {
    /// Basic constructor
    pub fn new<T: Into<String>>(s: T) -> Self {
        Group(s.into())
    }

    /// Retrieves underlying name.
    pub fn value(&self) -> &str {
        &self.0
    }
}

impl From<Group> for String {
    fn from(group: Group) -> Self {
        group.0
    }
}

impl ToBytes for Group {
    fn to_bytes(&self) -> Result<Vec<u8>, bytesrepr::Error> {
        self.0.to_bytes()
    }

    fn serialized_length(&self) -> usize {
        self.0.serialized_length()
    }
}

impl FromBytes for Group {
    fn from_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), bytesrepr::Error> {
        String::from_bytes(bytes).map(|(label, bytes)| (Group(label), bytes))
    }
}

/// Automatically incremented value for a contract version within a major `ProtocolVersion`.
pub type ContractVersion = u32;

/// Within each discrete major `ProtocolVersion`, contract version resets to this value.
pub const CONTRACT_INITIAL_VERSION: ContractVersion = 1;

/// Major element of `ProtocolVersion` a `ContractVersion` is compatible with.
pub type ProtocolVersionMajor = u32;

/// Major element of `ProtocolVersion` combined with `ContractVersion`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ContractVersionKey(ProtocolVersionMajor, ContractVersion);

impl ContractVersionKey {
    /// Returns a new instance of ContractVersionKey with provided values.
    pub fn new(
        protocol_version_major: ProtocolVersionMajor,
        contract_version: ContractVersion,
    ) -> Self {
        Self(protocol_version_major, contract_version)
    }

    /// Returns the major element of the protocol version this contract is compatible with.
    pub fn protocol_version_major(self) -> ProtocolVersionMajor {
        self.0
    }

    /// Returns the contract version within the protocol major version.
    pub fn contract_version(self) -> ContractVersion {
        self.1
    }
}

impl From<ContractVersionKey> for (ProtocolVersionMajor, ContractVersion) {
    fn from(contract_version_key: ContractVersionKey) -> Self {
        (contract_version_key.0, contract_version_key.1)
    }
}

/// Serialized length of `ContractVersionKey`.
pub const CONTRACT_VERSION_KEY_SERIALIZED_LENGTH: usize =
    U32_SERIALIZED_LENGTH + U32_SERIALIZED_LENGTH;

impl ToBytes for ContractVersionKey {
    fn to_bytes(&self) -> Result<Vec<u8>, bytesrepr::Error> {
        let mut ret = bytesrepr::unchecked_allocate_buffer(self);
        ret.append(&mut self.0.to_bytes()?);
        ret.append(&mut self.1.to_bytes()?);
        Ok(ret)
    }

    fn serialized_length(&self) -> usize {
        CONTRACT_VERSION_KEY_SERIALIZED_LENGTH
    }
}

impl FromBytes for ContractVersionKey {
    fn from_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), bytesrepr::Error> {
        let (major, rem): (u32, &[u8]) = FromBytes::from_bytes(bytes)?;
        let (contract, rem): (ContractVersion, &[u8]) = FromBytes::from_bytes(rem)?;
        Ok((ContractVersionKey::new(major, contract), rem))
    }
}

impl fmt::Display for ContractVersionKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}.{}", self.0, self.1)
    }
}

/// Collection of contract versions.
pub type ContractVersions = BTreeMap<ContractVersionKey, ContractHash>;

/// Collection of disabled contract versions. The runtime will not permit disabled
/// contract versions to be executed.
pub type DisabledVersions = BTreeSet<ContractVersionKey>;

/// Collection of named groups.
pub type Groups = BTreeMap<Group, BTreeSet<URef>>;

/// Contract definition, metadata, and security container.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContractPackage {
    /// Key used to add or disable versions
    access_key: URef,
    /// All versions (enabled & disabled)
    versions: ContractVersions,
    /// Disabled versions
    disabled_versions: DisabledVersions,
    /// Mapping maintaining the set of URefs associated with each "user
    /// group". This can be used to control access to methods in a particular
    /// version of the contract. A method is callable by any context which
    /// "knows" any of the URefs assoicated with the mthod's user group.
    groups: Groups,
}

impl ContractPackage {
    /// Create new `ContractPackage` (with no versions) from given access key.
    pub fn new(
        access_key: URef,
        versions: ContractVersions,
        disabled_versions: DisabledVersions,
        groups: Groups,
    ) -> Self {
        ContractPackage {
            access_key,
            versions,
            disabled_versions,
            groups,
        }
    }

    /// Get the access key for this contract.
    pub fn access_key(&self) -> URef {
        self.access_key
    }

    /// Get the mutable group definitions for this contract.
    pub fn groups_mut(&mut self) -> &mut Groups {
        &mut self.groups
    }

    /// Get the group definitions for this contract.
    pub fn groups(&self) -> &Groups {
        &self.groups
    }

    /// Adds new group to this contract.
    pub fn add_group(&mut self, group: Group, urefs: BTreeSet<URef>) {
        let v = self.groups.entry(group).or_insert_with(Default::default);
        v.extend(urefs)
    }

    /// Lookup the contract hash for a given contract version (if present)
    pub fn lookup_contract_hash(
        &self,
        contract_version_key: ContractVersionKey,
    ) -> Option<&ContractHash> {
        if !self.is_version_enabled(contract_version_key) {
            return None;
        }
        self.versions.get(&contract_version_key)
    }

    /// Checks if the given contract version exists and is available for use.
    pub fn is_version_enabled(&self, contract_version_key: ContractVersionKey) -> bool {
        !self.disabled_versions.contains(&contract_version_key)
            && self.versions.contains_key(&contract_version_key)
    }

    /// Insert a new contract version; the next sequential version number will be issued.
    pub fn insert_contract_version(
        &mut self,
        protocol_version_major: ProtocolVersionMajor,
        contract_hash: ContractHash,
    ) -> ContractVersionKey {
        let contract_version = self.next_contract_version_for(protocol_version_major);
        let key = ContractVersionKey::new(protocol_version_major, contract_version);
        self.versions.insert(key, contract_hash);
        key
    }

    /// Disable the contract version corresponding to the given hash (if it exists).
    pub fn disable_contract_version(&mut self, contract_hash: ContractHash) -> Result<(), Error> {
        let contract_version_key = self
            .versions
            .iter()
            .filter_map(|(k, v)| if *v == contract_hash { Some(*k) } else { None })
            .next()
            .ok_or(Error::ContractNotFound)?;

        if !self.disabled_versions.contains(&contract_version_key) {
            self.disabled_versions.insert(contract_version_key);
        }

        Ok(())
    }

    /// Returns reference to all of this contract's versions.
    pub fn versions(&self) -> &ContractVersions {
        &self.versions
    }

    /// Returns all of this contract's enabled contract versions.
    pub fn enabled_versions(&self) -> ContractVersions {
        let mut ret = ContractVersions::new();
        for version in &self.versions {
            if !self.is_version_enabled(*version.0) {
                continue;
            }
            ret.insert(*version.0, *version.1);
        }
        ret
    }

    /// Returns mutable reference to all of this contract's versions (enabled and disabled).
    pub fn versions_mut(&mut self) -> &mut ContractVersions {
        &mut self.versions
    }

    /// Consumes the object and returns all of this contract's versions (enabled and disabled).
    pub fn take_versions(self) -> ContractVersions {
        self.versions
    }

    /// Returns all of this contract's disabled versions.
    pub fn disabled_versions(&self) -> &DisabledVersions {
        &self.disabled_versions
    }

    /// Returns mut reference to all of this contract's disabled versions.
    pub fn disabled_versions_mut(&mut self) -> &mut DisabledVersions {
        &mut self.disabled_versions
    }

    /// Removes a group from this contract (if it exists).
    pub fn remove_group(&mut self, group: &Group) -> bool {
        self.groups.remove(group).is_some()
    }

    /// Gets the next available contract version for the given protocol version
    fn next_contract_version_for(&self, protocol_version: ProtocolVersionMajor) -> ContractVersion {
        let current_version = self
            .versions
            .keys()
            .rev()
            .find_map(|&contract_version_key| {
                if contract_version_key.protocol_version_major() == protocol_version {
                    Some(contract_version_key.contract_version())
                } else {
                    None
                }
            })
            .unwrap_or(0);

        current_version + 1
    }

    /// Return the contract version key for the newest enabled contract version.
    pub fn current_contract_version(&self) -> Option<ContractVersionKey> {
        match self.enabled_versions().keys().next_back() {
            Some(contract_version_key) => Some(*contract_version_key),
            None => None,
        }
    }

    /// Return the contract hash for the newest enabled contract version.
    pub fn current_contract_hash(&self) -> Option<ContractHash> {
        match self.enabled_versions().values().next_back() {
            Some(contract_hash) => Some(*contract_hash),
            None => None,
        }
    }
}

impl ToBytes for ContractPackage {
    fn to_bytes(&self) -> Result<Vec<u8>, bytesrepr::Error> {
        let mut result = bytesrepr::allocate_buffer(self)?;

        result.append(&mut self.access_key.to_bytes()?);
        result.append(&mut self.versions.to_bytes()?);
        result.append(&mut self.disabled_versions.to_bytes()?);
        result.append(&mut self.groups.to_bytes()?);

        Ok(result)
    }

    fn serialized_length(&self) -> usize {
        self.access_key.serialized_length()
            + self.versions.serialized_length()
            + self.disabled_versions.serialized_length()
            + self.groups.serialized_length()
    }
}

impl FromBytes for ContractPackage {
    fn from_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), bytesrepr::Error> {
        let (access_key, bytes) = URef::from_bytes(bytes)?;
        let (versions, bytes) = ContractVersions::from_bytes(bytes)?;
        let (disabled_versions, bytes) = DisabledVersions::from_bytes(bytes)?;
        let (groups, bytes) = Groups::from_bytes(bytes)?;
        let result = ContractPackage {
            access_key,
            versions,
            disabled_versions,
            groups,
        };

        Ok((result, bytes))
    }
}

/// Type alias for a container used inside [`EntryPoints`].
pub type EntryPointsMap = BTreeMap<String, EntryPoint>;

/// Collection of named entry points
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryPoints(EntryPointsMap);

impl Default for EntryPoints {
    fn default() -> Self {
        let mut entry_points = EntryPoints::new();
        let entry_point = EntryPoint::default();
        entry_points.add_entry_point(entry_point);
        entry_points
    }
}

impl ToBytes for EntryPoints {
    fn to_bytes(&self) -> Result<Vec<u8>, bytesrepr::Error> {
        self.0.to_bytes()
    }
    fn serialized_length(&self) -> usize {
        self.0.serialized_length()
    }
}

impl FromBytes for EntryPoints {
    fn from_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), bytesrepr::Error> {
        let (entry_points_map, rem) = EntryPointsMap::from_bytes(bytes)?;
        Ok((EntryPoints(entry_points_map), rem))
    }
}

impl EntryPoints {
    /// Creates empty instance of [`EntryPoints`].
    pub fn new() -> EntryPoints {
        EntryPoints(EntryPointsMap::new())
    }

    /// Adds new [`EntryPoint`].
    pub fn add_entry_point(&mut self, entry_point: EntryPoint) {
        self.0.insert(entry_point.name().to_string(), entry_point);
    }

    /// Checks if given [`EntryPoint`] exists.
    pub fn has_entry_point(&self, entry_point_name: &str) -> bool {
        self.0.contains_key(entry_point_name)
    }

    /// Gets an existing [`EntryPoint`] by its name.
    pub fn get(&self, entry_point_name: &str) -> Option<&EntryPoint> {
        self.0.get(entry_point_name)
    }

    /// Returns iterator for existing entry point names.
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.0.keys()
    }

    /// Takes all entry points.
    pub fn take_entry_points(self) -> Vec<EntryPoint> {
        self.0.into_iter().map(|(_name, value)| value).collect()
    }
}

impl From<Vec<EntryPoint>> for EntryPoints {
    fn from(entry_points: Vec<EntryPoint>) -> EntryPoints {
        let entries = entry_points
            .into_iter()
            .map(|entry_point| (String::from(entry_point.name()), entry_point))
            .collect();
        EntryPoints(entries)
    }
}

/// Collection of named keys
pub type NamedKeys = BTreeMap<String, Key>;

/// Methods and type signatures supported by a contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contract {
    contract_package_hash: ContractPackageHash,
    contract_wasm_hash: ContractWasmHash,
    named_keys: NamedKeys,
    entry_points: EntryPoints,
    protocol_version: ProtocolVersion,
}

impl From<Contract>
    for (
        ContractPackageHash,
        ContractWasmHash,
        NamedKeys,
        EntryPoints,
        ProtocolVersion,
    )
{
    fn from(contract: Contract) -> Self {
        (
            contract.contract_package_hash,
            contract.contract_wasm_hash,
            contract.named_keys,
            contract.entry_points,
            contract.protocol_version,
        )
    }
}

impl Contract {
    /// `Contract` constructor.
    pub fn new(
        contract_package_hash: ContractPackageHash,
        contract_wasm_hash: ContractWasmHash,
        named_keys: NamedKeys,
        entry_points: EntryPoints,
        protocol_version: ProtocolVersion,
    ) -> Self {
        Contract {
            contract_package_hash,
            contract_wasm_hash,
            named_keys,
            entry_points,
            protocol_version,
        }
    }

    /// Hash for accessing contract package
    pub fn contract_package_hash(&self) -> ContractPackageHash {
        self.contract_package_hash
    }

    /// Hash for accessing contract WASM
    pub fn contract_wasm_hash(&self) -> ContractWasmHash {
        self.contract_wasm_hash
    }

    /// Checks whether there is a method with the given name
    pub fn has_entry_point(&self, name: &str) -> bool {
        self.entry_points.has_entry_point(name)
    }

    /// Returns the type signature for the given `method`.
    pub fn entry_point(&self, method: &str) -> Option<&EntryPoint> {
        self.entry_points.get(method)
    }

    /// Get the protocol version this header is targeting.
    pub fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Adds new entry point
    pub fn add_entry_point<T: Into<String>>(&mut self, entry_point: EntryPoint) {
        self.entry_points.add_entry_point(entry_point);
    }

    /// Hash for accessing contract bytes
    pub fn contract_wasm_key(&self) -> Key {
        self.contract_wasm_hash.into()
    }

    /// Returns immutable reference to methods
    pub fn entry_points(&self) -> &EntryPoints {
        &self.entry_points
    }

    /// Takes `named_keys`
    pub fn take_named_keys(self) -> NamedKeys {
        self.named_keys
    }

    /// Returns a reference to `named_keys`
    pub fn named_keys(&self) -> &NamedKeys {
        &self.named_keys
    }

    /// Appends `keys` to `named_keys`
    pub fn named_keys_append(&mut self, keys: &mut NamedKeys) {
        self.named_keys.append(keys);
    }

    /// Removes given named key.
    pub fn remove_named_key(&mut self, key: &str) -> Option<Key> {
        self.named_keys.remove(key)
    }

    /// Determines if `Contract` is compatibile with a given `ProtocolVersion`.
    pub fn is_compatible_protocol_version(&self, protocol_version: ProtocolVersion) -> bool {
        self.protocol_version.value().major == protocol_version.value().major
    }
}

impl ToBytes for Contract {
    fn to_bytes(&self) -> Result<Vec<u8>, bytesrepr::Error> {
        let mut result = bytesrepr::allocate_buffer(self)?;
        result.append(&mut self.contract_package_hash.to_bytes()?);
        result.append(&mut self.contract_wasm_hash.to_bytes()?);
        result.append(&mut self.named_keys.to_bytes()?);
        result.append(&mut self.entry_points.to_bytes()?);
        result.append(&mut self.protocol_version.to_bytes()?);
        Ok(result)
    }

    fn serialized_length(&self) -> usize {
        ToBytes::serialized_length(&self.entry_points)
            + ToBytes::serialized_length(&self.contract_package_hash)
            + ToBytes::serialized_length(&self.contract_wasm_hash)
            + ToBytes::serialized_length(&self.protocol_version)
            + ToBytes::serialized_length(&self.named_keys)
    }
}

impl FromBytes for Contract {
    fn from_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), bytesrepr::Error> {
        let (contract_package_hash, bytes) = FromBytes::from_bytes(bytes)?;
        let (contract_wasm_hash, bytes) = FromBytes::from_bytes(bytes)?;
        let (named_keys, bytes) = NamedKeys::from_bytes(bytes)?;
        let (entry_points, bytes) = EntryPoints::from_bytes(bytes)?;
        let (protocol_version, bytes) = ProtocolVersion::from_bytes(bytes)?;
        Ok((
            Contract {
                contract_package_hash,
                contract_wasm_hash,
                named_keys,
                entry_points,
                protocol_version,
            },
            bytes,
        ))
    }
}

impl Default for Contract {
    fn default() -> Self {
        Contract {
            named_keys: NamedKeys::default(),
            entry_points: EntryPoints::default(),
            contract_wasm_hash: [0; KEY_HASH_LENGTH],
            contract_package_hash: [0; KEY_HASH_LENGTH],
            protocol_version: ProtocolVersion::V1_0_0,
        }
    }
}

/// Context of method execution
#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum EntryPointType {
    /// Runs as session code
    Session = 0,
    /// Runs within contract's context
    Contract = 1,
}

impl ToBytes for EntryPointType {
    fn to_bytes(&self) -> Result<Vec<u8>, bytesrepr::Error> {
        (*self as u8).to_bytes()
    }

    fn serialized_length(&self) -> usize {
        1
    }
}

impl FromBytes for EntryPointType {
    fn from_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), bytesrepr::Error> {
        let (value, bytes) = u8::from_bytes(bytes)?;
        match value {
            0 => Ok((EntryPointType::Session, bytes)),
            1 => Ok((EntryPointType::Contract, bytes)),
            _ => Err(bytesrepr::Error::Formatting),
        }
    }
}

/// Default name for an entry point
pub const DEFAULT_ENTRY_POINT_NAME: &str = "call";

/// Default name for an installer entry point
pub const ENTRY_POINT_NAME_INSTALL: &str = "install";

/// Default name for an upgrader entry point
pub const UPGRADE_ENTRY_POINT_NAME: &str = "upgrade";

/// Collection of entry point parameters.
pub type Parameters = Vec<Parameter>;

/// Type signature of a method. Order of arguments matter since can be
/// referenced by index as well as name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryPoint {
    name: String,
    args: Parameters,
    ret: CLType,
    access: EntryPointAccess,
    entry_point_type: EntryPointType,
}

impl From<EntryPoint> for (String, Parameters, CLType, EntryPointAccess, EntryPointType) {
    fn from(entry_point: EntryPoint) -> Self {
        (
            entry_point.name,
            entry_point.args,
            entry_point.ret,
            entry_point.access,
            entry_point.entry_point_type,
        )
    }
}

impl EntryPoint {
    /// `EntryPoint` constructor.
    pub fn new<T: Into<String>>(
        name: T,
        args: Parameters,
        ret: CLType,
        access: EntryPointAccess,
        entry_point_type: EntryPointType,
    ) -> Self {
        EntryPoint {
            name: name.into(),
            args,
            ret,
            access,
            entry_point_type,
        }
    }

    /// Create a default [`EntryPoint`] with specified name.
    pub fn default_with_name<T: Into<String>>(name: T) -> Self {
        EntryPoint {
            name: name.into(),
            ..Default::default()
        }
    }

    /// Get name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get access enum.
    pub fn access(&self) -> &EntryPointAccess {
        &self.access
    }

    /// Get the arguments for this method.
    pub fn args(&self) -> &[Parameter] {
        self.args.as_slice()
    }

    /// Get the return type.
    pub fn ret(&self) -> &CLType {
        &self.ret
    }

    /// Obtains entry point
    pub fn entry_point_type(&self) -> EntryPointType {
        self.entry_point_type
    }
}

impl Default for EntryPoint {
    /// constructor for a public session `EntryPoint` that takes no args and returns `Unit`
    fn default() -> Self {
        EntryPoint {
            name: DEFAULT_ENTRY_POINT_NAME.to_string(),
            args: Vec::new(),
            ret: CLType::Unit,
            access: EntryPointAccess::Public,
            entry_point_type: EntryPointType::Session,
        }
    }
}

impl ToBytes for EntryPoint {
    fn to_bytes(&self) -> Result<Vec<u8>, bytesrepr::Error> {
        let mut result = bytesrepr::allocate_buffer(self)?;
        result.append(&mut self.name.to_bytes()?);
        result.append(&mut self.args.to_bytes()?);
        self.ret.append_bytes(&mut result);
        result.append(&mut self.access.to_bytes()?);
        result.append(&mut self.entry_point_type.to_bytes()?);

        Ok(result)
    }

    fn serialized_length(&self) -> usize {
        self.name.serialized_length()
            + self.args.serialized_length()
            + self.ret.serialized_length()
            + self.access.serialized_length()
            + self.entry_point_type.serialized_length()
    }
}

impl FromBytes for EntryPoint {
    fn from_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), bytesrepr::Error> {
        let (name, bytes) = String::from_bytes(bytes)?;
        let (args, bytes) = Vec::<Parameter>::from_bytes(bytes)?;
        let (ret, bytes) = CLType::from_bytes(bytes)?;
        let (access, bytes) = EntryPointAccess::from_bytes(bytes)?;
        let (entry_point_type, bytes) = EntryPointType::from_bytes(bytes)?;

        Ok((
            EntryPoint {
                name,
                args,
                ret,
                access,
                entry_point_type,
            },
            bytes,
        ))
    }
}

/// Enum describing the possible access control options for a contract entry
/// point (method).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryPointAccess {
    /// Anyone can call this method (no access controls).
    Public,
    /// Only users from the listed groups may call this method. Note: if the
    /// list is empty then this method is not callable from outside the
    /// contract.
    Groups(Vec<Group>),
}

const ENTRYPOINTACCESS_PUBLIC_TAG: u8 = 1;
const ENTRYPOINTACCESS_GROUPS_TAG: u8 = 2;

impl EntryPointAccess {
    /// Constructor for access granted to only listed groups.
    pub fn groups(labels: &[&str]) -> Self {
        let list: Vec<Group> = labels.iter().map(|s| Group(String::from(*s))).collect();
        EntryPointAccess::Groups(list)
    }
}

impl ToBytes for EntryPointAccess {
    fn to_bytes(&self) -> Result<Vec<u8>, bytesrepr::Error> {
        let mut result = bytesrepr::allocate_buffer(self)?;

        match self {
            EntryPointAccess::Public => {
                result.push(ENTRYPOINTACCESS_PUBLIC_TAG);
            }
            EntryPointAccess::Groups(groups) => {
                result.push(ENTRYPOINTACCESS_GROUPS_TAG);
                result.append(&mut groups.to_bytes()?);
            }
        }
        Ok(result)
    }

    fn serialized_length(&self) -> usize {
        match self {
            EntryPointAccess::Public => 1,
            EntryPointAccess::Groups(groups) => 1 + groups.serialized_length(),
        }
    }
}

impl FromBytes for EntryPointAccess {
    fn from_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), bytesrepr::Error> {
        let (tag, bytes) = u8::from_bytes(bytes)?;

        match tag {
            ENTRYPOINTACCESS_PUBLIC_TAG => Ok((EntryPointAccess::Public, bytes)),
            ENTRYPOINTACCESS_GROUPS_TAG => {
                let (groups, bytes) = Vec::<Group>::from_bytes(bytes)?;
                let result = EntryPointAccess::Groups(groups);
                Ok((result, bytes))
            }
            _ => Err(bytesrepr::Error::Formatting),
        }
    }
}

/// Parameter to a method
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    name: String,
    cl_type: CLType,
}

impl Parameter {
    /// `Parameter` constructor.
    pub fn new<T: Into<String>>(name: T, cl_type: CLType) -> Self {
        Parameter {
            name: name.into(),
            cl_type,
        }
    }

    /// Get the type of this argument.
    pub fn cl_type(&self) -> &CLType {
        &self.cl_type
    }
}

impl From<Parameter> for (String, CLType) {
    fn from(parameter: Parameter) -> Self {
        (parameter.name, parameter.cl_type)
    }
}

impl ToBytes for Parameter {
    fn to_bytes(&self) -> Result<Vec<u8>, bytesrepr::Error> {
        let mut result = ToBytes::to_bytes(&self.name)?;
        self.cl_type.append_bytes(&mut result);

        Ok(result)
    }

    fn serialized_length(&self) -> usize {
        ToBytes::serialized_length(&self.name) + self.cl_type.serialized_length()
    }
}

impl FromBytes for Parameter {
    fn from_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), bytesrepr::Error> {
        let (name, bytes) = String::from_bytes(bytes)?;
        let (cl_type, bytes) = CLType::from_bytes(bytes)?;

        Ok((Parameter { name, cl_type }, bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AccessRights, URef};
    use alloc::borrow::ToOwned;

    fn make_contract_package() -> ContractPackage {
        let mut contract_package = ContractPackage::new(
            URef::new([0; 32], AccessRights::NONE),
            ContractVersions::default(),
            DisabledVersions::default(),
            Groups::default(),
        );

        // add groups
        {
            let group_urefs = {
                let mut ret = BTreeSet::new();
                ret.insert(URef::new([1; 32], AccessRights::READ));
                ret
            };

            contract_package
                .groups_mut()
                .insert(Group::new("Group 1"), group_urefs.clone());

            contract_package
                .groups_mut()
                .insert(Group::new("Group 2"), group_urefs);
        }

        // add entry_points
        let _entry_points = {
            let mut ret = BTreeMap::new();
            let entrypoint = EntryPoint::new(
                "method0".to_string(),
                vec![],
                CLType::U32,
                EntryPointAccess::groups(&["Group 2"]),
                EntryPointType::Session,
            );
            ret.insert(entrypoint.name().to_owned(), entrypoint);
            let entrypoint = EntryPoint::new(
                "method1".to_string(),
                vec![Parameter::new("Foo", CLType::U32)],
                CLType::U32,
                EntryPointAccess::groups(&["Group 1"]),
                EntryPointType::Session,
            );
            ret.insert(entrypoint.name().to_owned(), entrypoint);
            ret
        };

        let _contract_package_hash = [41; 32];
        let contract_hash = [42; 32];
        let _contract_wasm_hash = [43; 32];
        let _named_keys = NamedKeys::new();
        let protocol_version = ProtocolVersion::V1_0_0;

        contract_package.insert_contract_version(protocol_version.value().major, contract_hash);

        contract_package
    }

    #[test]
    fn next_contract_version() {
        let major = 1;
        let mut contract_package = ContractPackage::new(
            URef::new([0; 32], AccessRights::NONE),
            ContractVersions::default(),
            DisabledVersions::default(),
            Groups::default(),
        );
        assert_eq!(contract_package.next_contract_version_for(major), 1);

        let next_version = contract_package.insert_contract_version(major, [123; 32]);
        assert_eq!(next_version, ContractVersionKey::new(major, 1));
        assert_eq!(contract_package.next_contract_version_for(major), 2);
        let next_version_2 = contract_package.insert_contract_version(major, [124; 32]);
        assert_eq!(next_version_2, ContractVersionKey::new(major, 2));

        let major = 2;
        assert_eq!(contract_package.next_contract_version_for(major), 1);
        let next_version_3 = contract_package.insert_contract_version(major, [42; 32]);
        assert_eq!(next_version_3, ContractVersionKey::new(major, 1));
    }

    #[test]
    fn roundtrip_serialization() {
        let contract_package = make_contract_package();
        let bytes = contract_package.to_bytes().expect("should serialize");
        let (decoded_package, rem) =
            ContractPackage::from_bytes(&bytes).expect("should deserialize");
        assert_eq!(contract_package, decoded_package);
        assert_eq!(rem.len(), 0);
    }

    #[test]
    fn should_remove_group() {
        let mut contract_package = make_contract_package();

        assert!(!contract_package.remove_group(&Group::new("Non-existent group")));
        assert!(contract_package.remove_group(&Group::new("Group 1")));
        assert!(!contract_package.remove_group(&Group::new("Group 1"))); // Group no longer exists
    }

    #[test]
    fn should_disable_contract_version() {
        const CONTRACT_HASH: ContractHash = [123; 32];
        let mut contract_package = make_contract_package();

        assert_eq!(
            contract_package.disable_contract_version(CONTRACT_HASH),
            Err(Error::ContractNotFound),
            "should return contract not found error"
        );

        let next_version = contract_package.insert_contract_version(1, CONTRACT_HASH);
        assert!(
            contract_package.is_version_enabled(next_version),
            "version should exist and be enabled"
        );

        assert_eq!(
            contract_package.disable_contract_version(CONTRACT_HASH),
            Ok(()),
            "should be able to disable version"
        );

        assert_eq!(
            contract_package.lookup_contract_hash(next_version),
            None,
            "should not return disabled contract version"
        );

        assert!(
            !contract_package.is_version_enabled(next_version),
            "version should not be enabled"
        );
    }
}
