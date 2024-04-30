{-# LANGUAGE TemplateHaskell #-}

module Website.AjaxGoogleApis.Types where

import Data.Aeson
import Data.Aeson.TH
import qualified Data.Map.Strict as M
import Data.Text (Text)

data PackageId
  = PCesium
  | PD3
  | PDojo
  | PExtCore
  | PHammer
  | PIndefiniteObservable
  | PJQuery
  | PJQueryMobile
  | PJQueryUI
  | PList
  | PMaterialMotion
  | PModelViewer
  | PMootools
  | PMyanmarTools
  | PPrototype
  | PScriptAculoUs
  | PShakaPlayer
  | PSpf
  | PSwfobject
  | PThree
  | PWebFontLoader
  | PUnknown Text
  deriving (Show, Eq, Ord)

deriveJSON
  defaultOptions {sumEncoding = UntaggedValue, constructorTagModifier = camelTo2 '-' . tail}
  ''PackageId

data Package = Package
  { pkgId :: PackageId,
    pkgElementId :: Text,
    pkgName :: Text,
    pkgWebsite :: Text,
    pkgSnippets :: Maybe [Text],
    pkgNamedSnippets :: M.Map Text [Text],
    pkgVersions :: Maybe [Text],
    pkgNamedVersions :: M.Map Text [Text]
  }
  deriving (Show)

deriveJSON
  defaultOptions {fieldLabelModifier = camelTo2 '_' . drop 3}
  ''Package

data VersionInfo = VersionInfo
  { viVersion :: Text,
    viDir :: Maybe Text
  }
  deriving (Show)

deriveJSON
  defaultOptions {fieldLabelModifier = camelTo2 '_' . drop 2}
  ''VersionInfo