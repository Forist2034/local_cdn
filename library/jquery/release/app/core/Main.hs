module Main where

import Data.Aeson.Encode.Pretty
import qualified Data.ByteString.Lazy as LBS
import Source.JQuery.Core
import System.Environment

main :: IO ()
main = do
  [relDir, destPath] <- getArgs
  core <- addJQueryCoreDir relDir
  LBS.writeFile destPath (encodePretty core)
