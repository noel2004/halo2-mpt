// We require the Hardhat Runtime Environment explicitly here. This is optional
// but useful for running the script in a standalone fashion through `node <script>`.
//
// When running the script with `npx hardhat run <script>` you'll find the Hardhat
// Runtime Environment's members available in the global scope.
const hre = require("hardhat");
const deploy = require("./deploy.json")

async function main() {

    const Greeter = await hre.ethers.getContractFactory("Greeter");
    const addr = deploy.greeter
    if (!addr){
      throw 'no addr for greeter contract'
    }
    const greeter = Greeter.attach(addr)
    console.log('greet before', await greeter.retrieve())
    const setGreetingTx = await greeter.set_value_failing(new Date().getTime());
    try {
      await setGreetingTx.wait()
    }catch {
      console.log("fail")
    }
    
    console.log('greet after', await greeter.retrieve())

}

  // We recommend this pattern to be able to use async/await everywhere
  // and properly handle errors.
  main()
    .then(() => {
        process.exit(0)
    })
    .catch((error) => {
      console.error(error);
      process.exit(1);
    });
  