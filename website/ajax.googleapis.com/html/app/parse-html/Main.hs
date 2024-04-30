module Main (main) where

import Data.Aeson
import Data.Aeson.Encode.Pretty
import qualified Data.ByteString.Lazy as LBS
import qualified Data.Text.IO as TIO
import System.Environment
import System.FilePath
import Website.AjaxGoogleApis.Html
import Website.AjaxGoogleApis.Lib
import Zenacy.HTML

encodeFilePretty :: (ToJSON a) => FilePath -> a -> IO ()
encodeFilePretty p a = LBS.writeFile p (encodePretty' defConfig {confCompare = compare} a)

main :: IO ()
main = do
  [htmlSrc, descRoot] <- getArgs
  html <- htmlParseEasy <$> TIO.readFile htmlSrc
  let pkg = getPackages html
  encodeFilePretty (descRoot </> "packages.json") pkg
  let lib = toLibraries pkg
  encodeFilePretty (descRoot </> "npm.json") (libNpm lib)
  encodeFilePretty (descRoot </> "three.json") (libThree lib)
  encodeFilePretty (descRoot </> "jquery.json") (libJQuery lib)
  -- encodeFilePretty (descRoot </> "jquery-ui.json") (libJQueryUI lib)
  -- encodeFilePretty (descRoot </> "jquery-mobile.json") (libJQueryMobile lib)
  encodeFilePretty (descRoot </> "dojo.json") (libDojo lib)
