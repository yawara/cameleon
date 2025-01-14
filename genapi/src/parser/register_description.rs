/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use tracing::debug;

use crate::{
    builder::{CacheStoreBuilder, NodeStoreBuilder, ValueStoreBuilder},
    RegisterDescription,
};

use super::{
    elem_name::{
        MAJOR_VERSION, MINOR_VERSION, MODEL_NAME, PRODUCT_GUID, REGISTER_DESCRIPTION,
        SCHEMA_MAJOR_VERSION, SCHEMA_MINOR_VERSION, SCHEMA_SUB_MINOR_VERSION, STANDARD_NAME_SPCACE,
        SUB_MINOR_VERSION, TOOL_TIP, VENDOR_NAME, VERSION_GUID,
    },
    elem_type::convert_to_uint,
    xml, Parse,
};

impl Parse for RegisterDescription {
    #[tracing::instrument(level = "trace")]
    fn parse(
        node: &mut xml::Node,
        _: &mut impl NodeStoreBuilder,
        _: &mut impl ValueStoreBuilder,
        _: &mut impl CacheStoreBuilder,
    ) -> Self {
        debug!("start parsing `RegisterDescription`");
        debug_assert_eq!(node.tag_name(), REGISTER_DESCRIPTION);

        let model_name = node.attribute_of(MODEL_NAME).unwrap().into();
        let vendor_name = node.attribute_of(VENDOR_NAME).unwrap().into();
        let tooltip = node.attribute_of(TOOL_TIP).map(Into::into);
        let standard_name_space = node.attribute_of(STANDARD_NAME_SPCACE).unwrap().into();
        let schema_major_version =
            convert_to_uint(&node.attribute_of(SCHEMA_MAJOR_VERSION).unwrap());
        let schema_minor_version =
            convert_to_uint(&node.attribute_of(SCHEMA_MINOR_VERSION).unwrap());
        let schema_subminor_version =
            convert_to_uint(&node.attribute_of(SCHEMA_SUB_MINOR_VERSION).unwrap());
        let major_version = convert_to_uint(&node.attribute_of(MAJOR_VERSION).unwrap());
        let minor_version = convert_to_uint(&node.attribute_of(MINOR_VERSION).unwrap());
        let subminor_version = convert_to_uint(&node.attribute_of(SUB_MINOR_VERSION).unwrap());
        let product_guid = node.attribute_of(PRODUCT_GUID).unwrap().into();
        let version_guid = node.attribute_of(VERSION_GUID).unwrap().into();

        Self {
            model_name,
            vendor_name,
            tooltip,
            standard_name_space,
            schema_major_version,
            schema_minor_version,
            schema_subminor_version,
            major_version,
            minor_version,
            subminor_version,
            product_guid,
            version_guid,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::elem_type::StandardNameSpace;

    use super::{super::utils::tests::parse_default, *};

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_register_description() {
        let xml = r#"
        <RegisterDescription
          ModelName="CameleonModel"
          VendorName="CameleonVendor"
          StandardNameSpace="None"
          SchemaMajorVersion="1"
          SchemaMinorVersion="1"
          SchemaSubMinorVersion="0"
          MajorVersion="1"
          MinorVersion="2"
          SubMinorVersion="3"
          ToolTip="ToolTiptest"
          ProductGuid="01234567-0123-0123-0123-0123456789ab"
          VersionGuid="76543210-3210-3210-3210-ba9876543210"
          xmlns="http://www.genicam.org/GenApi/Version_1_0"
          xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
          xsi:schemaLocation="http://www.genicam.org/GenApi/Version_1_0 GenApiSchema.xsd">

            <Category Name="Root" NameSpace="Standard">
                <pFeature>MyNode</pFeature>
                <pFeature>MyInt</pFeature>
                <pFeature>MyIntReg</pFeature>
                <pFeature>MyMaskedIntReg</pFeature>
                <pFeature>MyBoolean</pFeature>
                <pFeature>MyCommand</pFeature>
                <pFeature>MyEnumeration</pFeature>
                <pFeature>MyFloat</pFeature>
                <pFeature>MyFloatReg</pFeature>
                <pFeature>MyString</pFeature>
                <pFeature>MyStringReg</pFeature>
                <pFeature>MyRegister</pFeature>
                <pFeature>MyConverter</pFeature>
                <pFeature>MyIntConverter</pFeature>
                <pFeature>MySwissKnife</pFeature>
                <pFeature>MyIntSwissKnife</pFeature>
                <pFeature>MyPort</pFeature>
                <pFeature>MyStructEntry</pFeature>
            </Category>

            <Node Name = "MyNode"></Node>

            <Integer Name="MyInt">
                <Value>10</Value>
            </Integer>

            <IntReg Name="MyIntReg">
              <Address>0x10000</Address>
              <pLength>LengthNode</pLength>
              <pPort>Device</pPort>
            </IntReg>

            <MaskedIntReg Name="MyMaskedIntReg">
              <Address>0x10000</Address>
              <Length>4</Length>
              <pPort>Device</pPort>
              <LSB>3</LSB>
              <MSB>7</MSB>
            </MaskedIntReg>

            <Boolean Name="MyBoolean">
                <pValue>Node</pValue>
                <OnValue>1</OnValue>
                <OffValue>0</OffValue>
            </Boolean>

            <Command Name="MyCommand">
                <pValue>Node</pValue>
                <CommandValue>10</CommandValue>
            </Command>

            <Enumeration Name="MyEnumeration">
                <EnumEntry Name="Entry0">
                    <Value>0</Value>
                    <NumericValue>1.0</NumericValue>
                    <NumericValue>10.0</NumericValue>
                    <IsSelfClearing>Yes</IsSelfClearing>
                </EnumEntry>
                <EnumEntry Name="Entry1">
                    <Value>1</Value>
                </EnumEntry>
                <pValue>MyNode</pValue>
            <PollingTime>10</PollingTime>
            </Enumeration>

            <Float Name="MyFloat">
                <Value>10.0</Value>
            </Float>

            <FloatReg Name="MyFloatReg">
              <Address>0x10000</Address>
              <Length>4</Length>
              <pPort>Device</pPort>
            </FloatReg>

            <String Name="MyString">
                <Streamable>Yes</Streamable>
                <Value>Immediate String</Value>
            </String>

            <StringReg Name="MyStringReg">
              <Address>100000</Address>
              <Length>128</Length>
              <pPort>Device</pPort>
            </StringReg>

            <Register Name="MyRegister">
              <Address>0x10000</Address>
              <Length>4</Length>
              <pPort>Device</pPort>
            </Register>

            <Converter Name="MyConverter">
                <pVariable Name="Var1">pValue1</pVariable>
                <pVariable Name="Var2">pValue2</pVariable>
                <FormulaTo>FROM*Var1/Var2</FormulaTo>
                <FormulaFrom>TO/Var1*Var2</FormulaFrom>
                <pValue>Target</pValue>
             </Converter>

            <IntConverter Name="MyIntConverter">
                <FormulaTo>FROM</FormulaTo>
                <FormulaFrom>TO</FormulaFrom>
                <pValue>Target</pValue>
             </IntConverter>

            <IntSwissKnife Name="MyIntSwissKnife">
                <pVariable Name="Var1">pValue1</pVariable>
                <pVariable Name="Var2">pValue2</pVariable>
                <Constant Name="Const">10</Constant>
                <Expression Name="ConstBy2">2.0*Const</Expression>
                <Formula>Var1+Var2+ConstBy2</Formula>
             </IntSwissKnife>

            <SwissKnife Name="MySwissKnife">
                <pVariable Name="Var1">pValue1</pVariable>
                <pVariable Name="Var2">pValue2</pVariable>
                <Constant Name="Const">INF</Constant>
                <Expression Name="ConstBy2">2.0*Const</Expression>
                <Formula>Var1+Var2+ConstBy2</Formula>
             </SwissKnife>

            <Port Name="MyPort">
                <ChunkID>Fd3219</ChunkID>
                <SwapEndianess>Yes</SwapEndianess>
            </Port>

            <StructReg Comment="Struct Entry Comment">
                <Address>0x10000</Address>
                <Length>4</Length>
                <pPort>Device</pPort>
                <Endianess>BigEndian</Endianess>

                <StructEntry Name="MyStructEntry">
                    <Bit>24</Bit>
                </StructEntry>
            </StructReg>

            <Group Comment="Nothing to say">
                <IntReg Name="RegImpl">
                  <Address>0x10000</Address>
                  <pLength>LengthNode</pLength>
                  <pPort>Device</pPort>
                </IntReg>
            </Group>


        </RegisterDescription>
        "#;

        let (reg_desc, ..): (RegisterDescription, _, _, _) = parse_default(xml);
        assert_eq!(reg_desc.model_name(), "CameleonModel");
        assert_eq!(reg_desc.vendor_name(), "CameleonVendor");
        assert_eq!(reg_desc.standard_name_space(), StandardNameSpace::None);
        assert_eq!(reg_desc.schema_major_version(), 1);
        assert_eq!(reg_desc.schema_minor_version(), 1);
        assert_eq!(reg_desc.schema_subminor_version(), 0);
        assert_eq!(reg_desc.major_version(), 1);
        assert_eq!(reg_desc.minor_version(), 2);
        assert_eq!(reg_desc.subminor_version(), 3);
        assert_eq!(
            reg_desc.product_guid(),
            "01234567-0123-0123-0123-0123456789ab"
        );
        assert_eq!(
            reg_desc.version_guid(),
            "76543210-3210-3210-3210-ba9876543210"
        );
    }
}
