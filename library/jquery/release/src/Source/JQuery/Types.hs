{-# LANGUAGE TemplateHaskell #-}

module Source.JQuery.Types where

import Crypto.Hash
import Data.Aeson as AE
import qualified Data.Aeson.Encoding as AE
import qualified Data.Aeson.Key as AEK
import Data.Aeson.TH
import qualified Data.ByteArray.Encoding as BA
import qualified Data.ByteString as BS
import Data.Text (Text)
import qualified Data.Text as T
import qualified Data.Text.Encoding as TE
import qualified Data.Text.Lazy as LT
import qualified Data.Text.Lazy.Builder as LTB
import qualified Data.Text.Lazy.Builder.Int as LTB

newtype Hash = Hash (Digest SHA256)
  deriving (Show)

hashToSri :: Hash -> Text
hashToSri (Hash d) = T.pack "sha256-" <> TE.decodeASCII (BA.convertToBase BA.Base64 d)

hashFile :: FilePath -> IO Hash
hashFile p = Hash . hash <$> BS.readFile p

instance ToJSON Hash where
  toJSON h = AE.String (hashToSri h)

data Version = Version
  { verMajor :: Word,
    verMinor :: Word,
    verPatch :: Word,
    verPreRelease :: Maybe Text
  }
  deriving (Show, Eq, Ord)

versionText :: Version -> Text
versionText v =
  LT.toStrict
    ( LTB.toLazyText
        ( mconcat
            [ LTB.decimal (verMajor v),
              LTB.singleton '.',
              LTB.decimal (verMinor v),
              LTB.singleton '.',
              LTB.decimal (verPatch v)
            ]
            <> case verPreRelease v of
              Just p -> LTB.singleton '-' <> LTB.fromText p
              Nothing -> mempty
        )
    )

instance ToJSON Version where
  toJSON v = AE.String (versionText v)

instance ToJSONKey Version where
  toJSONKey =
    ToJSONKeyText
      (AEK.fromText . versionText)
      (AE.text . versionText)

data File = File
  { fName :: Text,
    fUrl :: Text,
    fHash :: Hash
  }
  deriving (Show)

deriveToJSON defaultOptions {fieldLabelModifier = camelTo2 '_' . tail} ''File