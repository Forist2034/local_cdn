{-# LANGUAGE GeneralisedNewtypeDeriving #-}
{-# LANGUAGE TemplateHaskell #-}

module Source.JQuery.Core (Release (..), JQueryCore (..), addJQueryCoreDir) where

import Control.Monad
import Data.Aeson
import Data.Aeson.TH
import Data.Char (isAlphaNum)
import qualified Data.List as L
import qualified Data.Map.Strict as M
import Data.Maybe
import qualified Data.Text as T
import Source.JQuery.Types
import System.Directory (listDirectory)
import System.FilePath
import qualified Text.ParserCombinators.ReadP as P
import qualified Text.ParserCombinators.ReadPrec as Prec
import Text.Read
import qualified Text.Read.Lex as Lex

data Release = Release
  { relOriginal :: Maybe File,
    relMin :: Maybe File,
    relPack :: Maybe File,
    relSlim :: Maybe File,
    relSlimMin :: Maybe File,
    relModule :: Maybe File,
    relModuleMin :: Maybe File,
    relSlimModule :: Maybe File,
    relSlimModuleMin :: Maybe File
  }
  deriving (Show)

emptyRelease :: Release
emptyRelease =
  Release
    { relOriginal = Nothing,
      relMin = Nothing,
      relPack = Nothing,
      relSlim = Nothing,
      relSlimMin = Nothing,
      relModule = Nothing,
      relModuleMin = Nothing,
      relSlimModule = Nothing,
      relSlimModuleMin = Nothing
    }

deriveToJSON
  defaultOptions
    { fieldLabelModifier = camelTo2 '_' . drop 3,
      omitNothingFields = True
    }
  ''Release

newtype CoreVer = CoreVer Version

instance Read CoreVer where
  readPrec =
    do
      major <- Prec.lift Lex.readDecP
      void (Prec.lift (P.char '.'))
      minor <- Prec.lift Lex.readDecP
      (patch, release) <-
        (Prec.<++)
          ( do
              void (Prec.lift (P.char '.'))
              p <- readPrec
              release <-
                Prec.lift
                  ( (P.<++)
                      ( Just . T.pack
                          <$> (P.+++)
                            (P.munch1 isAlphaNum)
                            (P.char '-' >> P.munch1 isAlphaNum)
                      )
                      (pure Nothing)
                  )
              pure (p, release)
          )
          ( do
              release <-
                Prec.lift
                  ((P.<++) (Just . T.pack <$> P.munch1 isAlphaNum) (pure Nothing))
              pure (0, release)
          )
      pure (CoreVer (Version {verMajor = major, verMinor = minor, verPatch = patch, verPreRelease = release}))

newtype JQueryCore = JQueryCore (M.Map Version Release)
  deriving (Show, ToJSON)

addFile :: String -> File -> JQueryCore -> JQueryCore
addFile path file jq@(JQueryCore mp)
  | (p1, ".js") <- splitExtension path =
      let (p2, e1) = splitExtension p1
       in case e1 of
            ".min" -> case splitExtension p2 of
              (p3, ".slim") -> addRel (\v -> v {relSlimMin = Just file}) p3
              (p3, ".module") -> case splitExtension p3 of
                (p4, ".slim") -> addRel (\v -> v {relSlimModuleMin = Just file}) p4
                _ -> addRel (\v -> v {relModuleMin = Just file}) p3
              _ -> addRel (\v -> v {relMin = Just file}) p2
            ".pack" -> addRel (\v -> v {relPack = Just file}) p2
            ".slim" -> addRel (\v -> v {relSlim = Just file}) p2
            ".module" -> case splitExtensions p2 of
              (p3, ".slim") -> addRel (\v -> v {relSlimModule = Just file}) p3
              _ -> addRel (\v -> v {relModule = Just file}) p2
            _ -> addRel (\v -> v {relOriginal = Just file}) p1
  | otherwise = jq
  where
    addRel f p =
      case readMaybe p of
        Just (CoreVer v) ->
          JQueryCore (M.alter (Just . f . fromMaybe emptyRelease) v mp)
        Nothing -> jq

addJQueryCoreDir :: FilePath -> IO JQueryCore
addJQueryCoreDir fp =
  listDirectory fp
    >>= foldM
      ( \jq f -> case L.stripPrefix "jquery-" f of
          Just p -> do
            hsh <- hashFile (fp </> f)
            pure
              ( addFile
                  p
                  File
                    { fName = T.pack f,
                      fUrl = T.pack ("https://code.jquery.com/" ++ f),
                      fHash = hsh
                    }
                  jq
              )
          Nothing -> pure jq
      )
      (JQueryCore M.empty)
